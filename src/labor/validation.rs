//! Exhaustive persistence validation for cross-owner player labor references.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Energy, Volume};
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::energy::calculate_power_duration_ceiling;
use crate::equipment::resolve_equipment_capability;
use crate::maintenance::{
    ActiveConditionDurationError, calculate_usable_condition_after_active_ticks,
};
use crate::material::MaterialId;
use crate::registry::Registries;
use crate::survival::Vitality;

use super::power_physics::{
    calculate_metabolic_duration, metabolic_output_per_tick, resolve_manual_power_exertion,
};
use super::{
    PlayerWork, PlayerWorkResourceBudgetError, PlayerWorkState,
    calculate_player_work_resource_budget,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerWorkValidationError {
    WorkWithoutPlayer,
    ManualCraftJobMissing,
    ManualCraftProcessMismatch,
    MiningJobMissing,
    MiningJobNotWorking,
    MiningMethodMissing,
    ManualCraftMissingWork,
    MultiplePlayerJobs,
    MiningMissingWork,
    ManualPowerMethodMissing,
    ManualPowerEquipmentMissing,
    ManualPowerEquipmentDefinitionMismatch,
    ManualPowerEquipmentConditionMismatch,
    ManualPowerEquipmentMounted,
    ManualPowerDestinationMissing,
    ManualPowerDestinationDefinitionMismatch,
    ManualPowerCarrierMismatch,
    ManualPowerDestinationCannotAcceptEnergy,
    ManualPowerDestinationCapacityExceeded,
    ManualPowerEquipmentCapabilityMissing,
    ManualPowerEquipmentCapabilityKindMismatch,
    ManualPowerZeroPower,
    ManualPowerScheduleInvalid,
    ManualPowerDurationMismatch,
    ManualPowerConditionDuration(ActiveConditionDurationError),
    ManualPowerConditionMismatch,
    ManualPowerResourceDoubleBooked,
    ProspectingMethodMissing,
    ProspectingUnknownMaterial { material: MaterialId },
    ProspectingRegionVolumeOverflow,
    ProspectingRegionTooLarge { actual: u128, maximum: u128 },
    ProspectingScheduleInvalid,
    ProspectingDurationMismatch,
    PlayerDead,
    MetabolicCostOverflow,
    InsufficientMetabolicEnergy { available: Energy, required: Energy },
    HydrationCostOverflow,
    InsufficientHydration { available: Volume, required: Volume },
}

impl Display for PlayerWorkValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WorkWithoutPlayer => formatter.write_str("player work exists without a player"),
            Self::ManualCraftJobMissing => {
                formatter.write_str("player work references missing manual craft job")
            }
            Self::ManualCraftProcessMismatch => formatter
                .write_str("player work references a production job that is not manual crafting"),
            Self::MiningJobMissing => {
                formatter.write_str("player work references missing mining job")
            }
            Self::MiningJobNotWorking => {
                formatter.write_str("player work references mining that is no longer active")
            }
            Self::MiningMethodMissing => {
                formatter.write_str("player mining work references a missing authored method")
            }
            Self::ManualCraftMissingWork => {
                formatter.write_str("active manual crafting job does not own player labor")
            }
            Self::MultiplePlayerJobs => {
                formatter.write_str("more than one active job requires exclusive player labor")
            }
            Self::MiningMissingWork => {
                formatter.write_str("working mining job does not own player labor")
            }
            Self::ManualPowerMethodMissing => {
                formatter.write_str("manual power work references missing authored method")
            }
            Self::ManualPowerEquipmentMissing => {
                formatter.write_str("manual power work references missing equipment")
            }
            Self::ManualPowerEquipmentDefinitionMismatch => formatter
                .write_str("manual power equipment definition disagrees with its persisted trace"),
            Self::ManualPowerEquipmentConditionMismatch => formatter
                .write_str("manual power equipment condition disagrees with its persisted trace"),
            Self::ManualPowerEquipmentMounted => {
                formatter.write_str("manual power work requires portable unmounted equipment")
            }
            Self::ManualPowerDestinationMissing => {
                formatter.write_str("manual power work references missing energy destination")
            }
            Self::ManualPowerDestinationDefinitionMismatch => formatter.write_str(
                "manual power destination definition disagrees with its persisted trace",
            ),
            Self::ManualPowerCarrierMismatch => formatter
                .write_str("manual power destination carrier disagrees with its authored method"),
            Self::ManualPowerDestinationCannotAcceptEnergy => formatter
                .write_str("manual power destination has no authored input-power capability"),
            Self::ManualPowerDestinationCapacityExceeded => {
                formatter.write_str("manual power output exceeds remaining destination capacity")
            }
            Self::ManualPowerEquipmentCapabilityMissing => {
                formatter.write_str("manual power equipment lacks its authored power capability")
            }
            Self::ManualPowerEquipmentCapabilityKindMismatch => {
                formatter.write_str("manual power equipment capability is not a Power value")
            }
            Self::ManualPowerZeroPower => {
                formatter.write_str("manual power work persists zero usable transfer power")
            }
            Self::ManualPowerScheduleInvalid => {
                formatter.write_str("manual power work has an invalid persisted schedule")
            }
            Self::ManualPowerDurationMismatch => {
                formatter.write_str("manual power duration disagrees with current authored physics")
            }
            Self::ManualPowerConditionDuration(error) => write!(
                formatter,
                "manual power work exceeds equipment condition lifetime: {error}"
            ),
            Self::ManualPowerConditionMismatch => formatter
                .write_str("manual power condition outcome disagrees with current authored wear"),
            Self::ManualPowerResourceDoubleBooked => formatter.write_str(
                "manual power equipment or destination is simultaneously owned elsewhere",
            ),
            Self::ProspectingMethodMissing => {
                formatter.write_str("player prospecting work references a missing authored method")
            }
            Self::ProspectingUnknownMaterial { material } => write!(
                formatter,
                "player prospecting work references unknown material {}",
                material.value()
            ),
            Self::ProspectingRegionVolumeOverflow => {
                formatter.write_str("player prospecting region voxel count overflowed")
            }
            Self::ProspectingRegionTooLarge { actual, maximum } => write!(
                formatter,
                "player prospecting region contains {actual} voxels but method allows at most {maximum}"
            ),
            Self::ProspectingScheduleInvalid => {
                formatter.write_str("player prospecting work has an invalid persisted schedule")
            }
            Self::ProspectingDurationMismatch => formatter
                .write_str("player prospecting duration disagrees with its authored method"),
            Self::PlayerDead => {
                formatter.write_str("player-owned work remains active for a dead player")
            }
            Self::MetabolicCostOverflow => formatter.write_str(
                "remaining player-work metabolic cost exceeds authoritative energy range",
            ),
            Self::InsufficientMetabolicEnergy {
                available,
                required,
            } => write!(
                formatter,
                "player work needs {} nJ to finish but player retains only {} nJ metabolic energy",
                required.nanojoules(),
                available.nanojoules()
            ),
            Self::HydrationCostOverflow => formatter.write_str(
                "remaining player-work hydration cost exceeds authoritative volume range",
            ),
            Self::InsufficientHydration {
                available,
                required,
            } => write!(
                formatter,
                "player work needs {} uL hydration to finish but player retains only {} uL",
                required.microliters(),
                available.microliters()
            ),
        }
    }
}

impl Error for PlayerWorkValidationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::ManualPowerConditionDuration(error) => Some(error),
            Self::WorkWithoutPlayer
            | Self::ManualCraftJobMissing
            | Self::ManualCraftProcessMismatch
            | Self::MiningJobMissing
            | Self::MiningJobNotWorking
            | Self::MiningMethodMissing
            | Self::ManualCraftMissingWork
            | Self::MultiplePlayerJobs
            | Self::MiningMissingWork
            | Self::ManualPowerMethodMissing
            | Self::ManualPowerEquipmentMissing
            | Self::ManualPowerEquipmentDefinitionMismatch
            | Self::ManualPowerEquipmentConditionMismatch
            | Self::ManualPowerEquipmentMounted
            | Self::ManualPowerDestinationMissing
            | Self::ManualPowerDestinationDefinitionMismatch
            | Self::ManualPowerCarrierMismatch
            | Self::ManualPowerDestinationCannotAcceptEnergy
            | Self::ManualPowerDestinationCapacityExceeded
            | Self::ManualPowerEquipmentCapabilityMissing
            | Self::ManualPowerEquipmentCapabilityKindMismatch
            | Self::ManualPowerZeroPower
            | Self::ManualPowerScheduleInvalid
            | Self::ManualPowerDurationMismatch
            | Self::ManualPowerConditionMismatch
            | Self::ManualPowerResourceDoubleBooked
            | Self::ProspectingMethodMissing
            | Self::ProspectingUnknownMaterial { .. }
            | Self::ProspectingRegionVolumeOverflow
            | Self::ProspectingRegionTooLarge { .. }
            | Self::ProspectingScheduleInvalid
            | Self::ProspectingDurationMismatch
            | Self::PlayerDead
            | Self::MetabolicCostOverflow
            | Self::InsufficientMetabolicEnergy { .. }
            | Self::HydrationCostOverflow
            | Self::InsufficientHydration { .. } => None,
        }
    }
}

pub(crate) fn validate_loaded_player_work(
    registries: &Registries,
    state: &AppState,
    work_state: &PlayerWorkState,
) -> Result<(), PlayerWorkValidationError> {
    let crafting = registries.crafting();
    let manual_jobs = state
        .production()
        .jobs()
        .filter(|job| job.suspension().is_none() && crafting.get_manual(job.process()).is_some())
        .map(|job| job.id())
        .collect::<Vec<_>>();
    let mining_jobs = state
        .mining()
        .jobs()
        .filter(|job| job.is_working())
        .map(|job| job.id())
        .collect::<Vec<_>>();
    if manual_jobs.len() + mining_jobs.len() > 1 {
        return Err(PlayerWorkValidationError::MultiplePlayerJobs);
    }
    let Some(work) = work_state.active() else {
        if !manual_jobs.is_empty() {
            return Err(PlayerWorkValidationError::ManualCraftMissingWork);
        }
        if !mining_jobs.is_empty() {
            return Err(PlayerWorkValidationError::MiningMissingWork);
        }
        return Ok(());
    };
    let player = state
        .survival()
        .player()
        .copied()
        .ok_or(PlayerWorkValidationError::WorkWithoutPlayer)?;
    if player.vitality() == Vitality::ZERO {
        return Err(PlayerWorkValidationError::PlayerDead);
    }
    match work {
        PlayerWork::ManualCraft { job } => {
            let Some(record) = state.production().get_job(job) else {
                return Err(PlayerWorkValidationError::ManualCraftJobMissing);
            };
            let Some(definition) = crafting.get_manual(record.process()) else {
                return Err(PlayerWorkValidationError::ManualCraftProcessMismatch);
            };
            if manual_jobs.as_slice() != [job] {
                return Err(PlayerWorkValidationError::ManualCraftMissingWork);
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
                                    "runtime invariant broken: running manual craft job is already due"
                                )
                            }),
                    )
                });
            validate_remaining_resources(
                registries,
                player.metabolic_energy(),
                player.hydration(),
                definition.exertion(),
                remaining,
            )?;
        }
        PlayerWork::Mining { job } => {
            let Some(record) = state.mining().get_job(job) else {
                return Err(PlayerWorkValidationError::MiningJobMissing);
            };
            if !record.is_working() {
                return Err(PlayerWorkValidationError::MiningJobNotWorking);
            }
            if mining_jobs.as_slice() != [job] {
                return Err(PlayerWorkValidationError::MiningMissingWork);
            }
            let method = registries
                .mining()
                .get_method(record.method())
                .ok_or(PlayerWorkValidationError::MiningMethodMissing)?;
            validate_remaining_resources(
                registries,
                player.metabolic_energy(),
                player.hydration(),
                method.exertion(),
                TickSpan::new(record.completes_at().value() - state.tick().value()),
            )?;
        }
        PlayerWork::ManualPower { work } => {
            if !manual_jobs.is_empty() || !mining_jobs.is_empty() {
                return Err(PlayerWorkValidationError::MultiplePlayerJobs);
            }
            let method = registries
                .labor()
                .get_manual_power(work.method())
                .copied()
                .ok_or(PlayerWorkValidationError::ManualPowerMethodMissing)?;
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
            let transfer_power =
                std::cmp::min(equipment_power, energy_definition.max_input_power());
            if transfer_power.is_zero() {
                return Err(PlayerWorkValidationError::ManualPowerZeroPower);
            }
            if work.started_at() > state.tick()
                || work.completes_at() <= state.tick()
                || work.completes_at() <= work.started_at()
            {
                return Err(PlayerWorkValidationError::ManualPowerScheduleInvalid);
            }
            let stored_duration =
                TickSpan::new(work.completes_at().value() - work.started_at().value());
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
            let metabolic_duration =
                calculate_metabolic_duration(work.output().energy(), metabolic_output)
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
                player.metabolic_energy(),
                player.hydration(),
                exertion,
                TickSpan::new(remaining_ticks),
            )?;
        }
        PlayerWork::Prospecting { work } => {
            if !manual_jobs.is_empty() || !mining_jobs.is_empty() {
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
            let stored_duration =
                TickSpan::new(work.completes_at().value() - work.started_at().value());
            if stored_duration != method.duration() {
                return Err(PlayerWorkValidationError::ProspectingDurationMismatch);
            }
            let remaining_ticks = work.completes_at().value() - state.tick().value();
            validate_remaining_resources(
                registries,
                player.metabolic_energy(),
                player.hydration(),
                method.exertion(),
                TickSpan::new(remaining_ticks),
            )?;
        }
    }
    Ok(())
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
