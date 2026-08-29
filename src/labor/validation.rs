//! Exhaustive persistence validation for cross-owner player labor references.

use crate::core::quantity::{Energy, Power, Volume};
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::energy::calculate_power_duration_ceiling;
use crate::equipment::resolve_equipment_capability;
use crate::maintenance::calculate_usable_condition_after_active_ticks;
use crate::registry::Registries;
use crate::survival::{SurvivalExertion, Vitality};

use super::power_physics::{
    calculate_metabolic_duration, metabolic_output_per_tick, resolve_manual_power_exertion,
};
use super::{
    ManualPowerDefinition, ManualPowerWork, PlayerWork, PlayerWorkResourceBudgetError,
    PlayerWorkState, ProspectingWork, calculate_player_work_resource_budget,
};

mod direct_consumption;
mod error;

use direct_consumption::{validate_drinking_work, validate_eating_work};
pub use error::PlayerWorkValidationError;

struct ActivePlayerJobs {
    manual_production: Vec<crate::production::ProductionJobId>,
    mining: Vec<crate::mining::MiningJobId>,
}

impl ActivePlayerJobs {
    fn has_any(&self) -> bool {
        !self.manual_production.is_empty() || !self.mining.is_empty()
    }
}

pub(crate) fn validate_loaded_player_work(
    registries: &Registries,
    state: &AppState,
    work_state: &PlayerWorkState,
) -> Result<(), PlayerWorkValidationError> {
    let active_jobs = collect_active_player_jobs(registries, state);
    if active_jobs.manual_production.len() + active_jobs.mining.len() > 1 {
        return Err(PlayerWorkValidationError::MultiplePlayerJobs);
    }
    let Some(work) = work_state.active() else {
        return validate_idle_player_work(&active_jobs);
    };
    let player = state
        .survival()
        .player()
        .copied()
        .ok_or(PlayerWorkValidationError::WorkWithoutPlayer)?;
    if player.vitality() == Vitality::ZERO
        && !matches!(
            work,
            PlayerWork::Eating { .. } | PlayerWork::Drinking { .. }
        )
    {
        return Err(PlayerWorkValidationError::PlayerDead);
    }
    let available_energy = player.metabolic_energy();
    let available_hydration = player.hydration();
    match work {
        PlayerWork::ManualProduction { job } => validate_manual_production_work(
            registries,
            state,
            &active_jobs,
            job,
            available_energy,
            available_hydration,
        ),
        PlayerWork::Mining { job } => validate_mining_work(
            registries,
            state,
            &active_jobs,
            job,
            available_energy,
            available_hydration,
        ),
        PlayerWork::ManualPower { work } => validate_manual_power_work(
            registries,
            state,
            &active_jobs,
            work,
            available_energy,
            available_hydration,
        ),
        PlayerWork::Prospecting { work } => validate_prospecting_work(
            registries,
            state,
            &active_jobs,
            work,
            available_energy,
            available_hydration,
        ),
        PlayerWork::Eating { work } => validate_eating_work(registries, state, &active_jobs, work),
        PlayerWork::Drinking { work } => {
            validate_drinking_work(registries, state, &active_jobs, work)
        }
    }
}

fn collect_active_player_jobs(registries: &Registries, state: &AppState) -> ActivePlayerJobs {
    let manual_production = state
        .production()
        .jobs()
        .filter(|job| {
            job.suspension().is_none()
                && registries.manual_process_exertion(job.process()).is_some()
        })
        .map(|job| job.id())
        .collect::<Vec<_>>();
    let mining = state
        .mining()
        .jobs()
        .filter(|job| job.is_working())
        .map(|job| job.id())
        .collect::<Vec<_>>();
    ActivePlayerJobs {
        manual_production,
        mining,
    }
}

fn validate_idle_player_work(
    active_jobs: &ActivePlayerJobs,
) -> Result<(), PlayerWorkValidationError> {
    if !active_jobs.manual_production.is_empty() {
        return Err(PlayerWorkValidationError::ManualProductionMissingWork);
    }
    if !active_jobs.mining.is_empty() {
        return Err(PlayerWorkValidationError::MiningMissingWork);
    }
    Ok(())
}

fn validate_manual_production_work(
    registries: &Registries,
    state: &AppState,
    active_jobs: &ActivePlayerJobs,
    job: crate::production::ProductionJobId,
    available_energy: Energy,
    available_hydration: Volume,
) -> Result<(), PlayerWorkValidationError> {
    let Some(record) = state.production().get_job(job) else {
        return Err(PlayerWorkValidationError::ManualProductionJobMissing);
    };
    let Some(exertion) = registries.manual_process_exertion(record.process()) else {
        return Err(PlayerWorkValidationError::ManualProductionProcessMismatch);
    };
    if active_jobs.manual_production.as_slice() != [job] {
        return Err(PlayerWorkValidationError::ManualProductionMissingWork);
    }
    let remaining = record
        .suspension()
        .map(|suspension| suspension.remaining_active_time())
        .unwrap_or_else(|| {
            TickSpan::new(
                record
                    .completes_at()
                    .value()
                    .checked_sub(state.tick().value())
                    .unwrap_or_else(|| {
                        panic!(
                            "runtime invariant broken: running manual production job is already due"
                        )
                    }),
            )
        });
    validate_remaining_resources(
        registries,
        available_energy,
        available_hydration,
        exertion,
        remaining,
    )
}

fn validate_mining_work(
    registries: &Registries,
    state: &AppState,
    active_jobs: &ActivePlayerJobs,
    job: crate::mining::MiningJobId,
    available_energy: Energy,
    available_hydration: Volume,
) -> Result<(), PlayerWorkValidationError> {
    let Some(record) = state.mining().get_job(job) else {
        return Err(PlayerWorkValidationError::MiningJobMissing);
    };
    if !record.is_working() {
        return Err(PlayerWorkValidationError::MiningJobNotWorking);
    }
    if active_jobs.mining.as_slice() != [job] {
        return Err(PlayerWorkValidationError::MiningMissingWork);
    }
    let method = registries
        .mining()
        .get_method(record.method())
        .ok_or(PlayerWorkValidationError::MiningMethodMissing)?;
    validate_remaining_resources(
        registries,
        available_energy,
        available_hydration,
        method.exertion(),
        TickSpan::new(record.completes_at().value() - state.tick().value()),
    )
}

fn validate_manual_power_work(
    registries: &Registries,
    state: &AppState,
    active_jobs: &ActivePlayerJobs,
    work: ManualPowerWork,
    available_energy: Energy,
    available_hydration: Volume,
) -> Result<(), PlayerWorkValidationError> {
    if active_jobs.has_any() {
        return Err(PlayerWorkValidationError::MultiplePlayerJobs);
    }
    let method = registries
        .labor()
        .get_manual_power(work.method())
        .copied()
        .ok_or(PlayerWorkValidationError::ManualPowerMethodMissing)?;
    let transfer_power = validate_manual_power_bindings(registries, state, work, method)?;
    let (required_duration, exertion) =
        validate_manual_power_schedule(registries, state, work, method, transfer_power)?;
    let required_condition = calculate_usable_condition_after_active_ticks(
        method.condition_wear_ppm_per_active_tick(),
        work.equipment_trace().condition(),
        required_duration,
    )
    .map_err(PlayerWorkValidationError::ManualPowerConditionDuration)?;
    if work.condition_after() != required_condition {
        return Err(PlayerWorkValidationError::ManualPowerConditionMismatch);
    }
    let remaining_ticks = work.completes_at().value() - state.tick().value();
    validate_remaining_resources(
        registries,
        available_energy,
        available_hydration,
        exertion,
        TickSpan::new(remaining_ticks),
    )
}

fn validate_manual_power_bindings(
    registries: &Registries,
    state: &AppState,
    work: ManualPowerWork,
    method: ManualPowerDefinition,
) -> Result<Power, PlayerWorkValidationError> {
    let equipment = state
        .equipment()
        .get_equipment(work.equipment())
        .ok_or(PlayerWorkValidationError::ManualPowerEquipmentMissing)?;
    if equipment.definition() != work.equipment_trace().definition() {
        return Err(PlayerWorkValidationError::ManualPowerEquipmentDefinitionMismatch);
    }
    if equipment.condition() != work.equipment_trace().condition() {
        return Err(PlayerWorkValidationError::ManualPowerEquipmentConditionMismatch);
    }
    if equipment.supported_by().is_some() {
        return Err(PlayerWorkValidationError::ManualPowerEquipmentMounted);
    }
    if state
        .production()
        .get_equipment_occupant(work.equipment())
        .is_some()
        || state
            .mining()
            .get_equipment_occupant(work.equipment())
            .is_some()
        || state
            .production()
            .get_energy_occupant(work.destination())
            .is_some()
    {
        return Err(PlayerWorkValidationError::ManualPowerResourceDoubleBooked);
    }
    let destination = state
        .energy()
        .get_store(work.destination())
        .ok_or(PlayerWorkValidationError::ManualPowerDestinationMissing)?;
    if destination.definition() != work.output().definition() {
        return Err(PlayerWorkValidationError::ManualPowerDestinationDefinitionMismatch);
    }
    let energy_definition = registries
        .energy()
        .get_store(destination.definition())
        .ok_or(PlayerWorkValidationError::ManualPowerDestinationDefinitionMismatch)?;
    if energy_definition.carrier() != method.carrier()
        || work.output().carrier() != method.carrier()
    {
        return Err(PlayerWorkValidationError::ManualPowerCarrierMismatch);
    }
    if energy_definition.max_input_power().is_zero() {
        return Err(PlayerWorkValidationError::ManualPowerDestinationCannotAcceptEnergy);
    }
    let stored_after = destination
        .stored()
        .checked_add(work.output().energy())
        .ok_or(PlayerWorkValidationError::ManualPowerDestinationCapacityExceeded)?;
    if stored_after > energy_definition.capacity() {
        return Err(PlayerWorkValidationError::ManualPowerDestinationCapacityExceeded);
    }
    let equipment_definition = registries
        .equipment()
        .get_equipment(equipment.definition())
        .ok_or(PlayerWorkValidationError::ManualPowerEquipmentDefinitionMismatch)?;
    let capability = resolve_equipment_capability(
        equipment_definition,
        equipment.condition(),
        method.power_capability(),
    )
    .ok_or(PlayerWorkValidationError::ManualPowerEquipmentCapabilityMissing)?;
    let crate::capability::CapabilityValue::Power(equipment_power) = capability else {
        return Err(PlayerWorkValidationError::ManualPowerEquipmentCapabilityKindMismatch);
    };
    let transfer_power = std::cmp::min(equipment_power, energy_definition.max_input_power());
    if transfer_power.is_zero() {
        return Err(PlayerWorkValidationError::ManualPowerZeroPower);
    }
    Ok(transfer_power)
}

fn validate_manual_power_schedule(
    registries: &Registries,
    state: &AppState,
    work: ManualPowerWork,
    method: ManualPowerDefinition,
    transfer_power: Power,
) -> Result<(TickSpan, SurvivalExertion), PlayerWorkValidationError> {
    if work.started_at() > state.tick()
        || work.completes_at() <= state.tick()
        || work.completes_at() <= work.started_at()
    {
        return Err(PlayerWorkValidationError::ManualPowerScheduleInvalid);
    }
    let stored_duration = TickSpan::new(work.completes_at().value() - work.started_at().value());
    let power_duration = calculate_power_duration_ceiling(
        transfer_power,
        work.output().energy(),
        registries.core().physical_tick_duration(),
    )
    .map_err(|_error| PlayerWorkValidationError::ManualPowerDurationMismatch)?;
    let metabolic_output = metabolic_output_per_tick(
        method.maximum_exertion().energy_cost_per_tick(),
        method.metabolic_efficiency_ppm(),
    );
    let metabolic_duration = calculate_metabolic_duration(work.output().energy(), metabolic_output)
        .map_err(|_error| PlayerWorkValidationError::ManualPowerDurationMismatch)?;
    let required_duration = std::cmp::max(power_duration, metabolic_duration);
    if stored_duration != required_duration {
        return Err(PlayerWorkValidationError::ManualPowerDurationMismatch);
    }
    let exertion = resolve_manual_power_exertion(
        work.output().energy(),
        stored_duration,
        method.maximum_exertion(),
        method.metabolic_efficiency_ppm(),
    )
    .map_err(|_error| PlayerWorkValidationError::ManualPowerDurationMismatch)?;
    Ok((required_duration, exertion))
}

fn validate_prospecting_work(
    registries: &Registries,
    state: &AppState,
    active_jobs: &ActivePlayerJobs,
    work: ProspectingWork,
    available_energy: Energy,
    available_hydration: Volume,
) -> Result<(), PlayerWorkValidationError> {
    if active_jobs.has_any() {
        return Err(PlayerWorkValidationError::MultiplePlayerJobs);
    }
    let method = registries
        .labor()
        .get_prospecting(work.method())
        .copied()
        .ok_or(PlayerWorkValidationError::ProspectingMethodMissing)?;
    if registries
        .materials()
        .get_material(work.material())
        .is_none()
    {
        return Err(PlayerWorkValidationError::ProspectingUnknownMaterial {
            material: work.material(),
        });
    }
    let region_voxels = work
        .region()
        .voxel_count()
        .ok_or(PlayerWorkValidationError::ProspectingRegionVolumeOverflow)?;
    if region_voxels > method.maximum_region_voxels() {
        return Err(PlayerWorkValidationError::ProspectingRegionTooLarge {
            actual: region_voxels,
            maximum: method.maximum_region_voxels(),
        });
    }
    if work.started_at() > state.tick()
        || work.completes_at() <= state.tick()
        || work.completes_at() <= work.started_at()
    {
        return Err(PlayerWorkValidationError::ProspectingScheduleInvalid);
    }
    let stored_duration = TickSpan::new(work.completes_at().value() - work.started_at().value());
    if stored_duration != method.duration() {
        return Err(PlayerWorkValidationError::ProspectingDurationMismatch);
    }
    let remaining_ticks = work.completes_at().value() - state.tick().value();
    validate_remaining_resources(
        registries,
        available_energy,
        available_hydration,
        method.exertion(),
        TickSpan::new(remaining_ticks),
    )
}

fn validate_remaining_resources(
    registries: &Registries,
    available_energy: Energy,
    available_hydration: Volume,
    exertion: crate::survival::SurvivalExertion,
    duration: TickSpan,
) -> Result<(), PlayerWorkValidationError> {
    let budget = calculate_player_work_resource_budget(
        registries.survival().physiology(),
        exertion,
        duration,
    )
    .map_err(|error| match error {
        PlayerWorkResourceBudgetError::EnergyOverflow => {
            PlayerWorkValidationError::MetabolicCostOverflow
        }
        PlayerWorkResourceBudgetError::HydrationOverflow => {
            PlayerWorkValidationError::HydrationCostOverflow
        }
    })?;
    if available_energy < budget.metabolic_energy() {
        return Err(PlayerWorkValidationError::InsufficientMetabolicEnergy {
            available: available_energy,
            required: budget.metabolic_energy(),
        });
    }
    if available_hydration < budget.hydration() {
        return Err(PlayerWorkValidationError::InsufficientHydration {
            available: available_hydration,
            required: budget.hydration(),
        });
    }
    Ok(())
}
