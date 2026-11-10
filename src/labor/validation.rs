//! Exhaustive persistence validation for cross-owner player labor references.

use crate::core::quantity::{Energy, Volume};
use crate::core::state::AppState;
use crate::core::time::{SimulationTick, TickSpan};
use crate::logistics::{
    player_body_has_fixed_floor, player_body_uses_generated_world, validate_player_stockpile_access,
};
use crate::registry::Registries;
use crate::spatial::HorizontalVelocity;
use crate::survival::{SurvivalExertion, Vitality};

use super::{
    PlayerWork, PlayerWorkResourceBudgetError, PlayerWorkState,
    calculate_player_work_resource_budget,
};

mod block_excavation;
mod direct_consumption;
mod equipment_maintenance;
mod error;
mod manual_power;
mod prospecting;
mod storage_enclosure_dismantling;
mod structural;
mod surface_gathering;
mod tree_felling;

use block_excavation::validate_block_excavation_work;
use direct_consumption::{
    validate_direct_consumption_binding, validate_drinking_work, validate_eating_work,
};
use equipment_maintenance::validate_equipment_maintenance_work;
pub use error::PlayerWorkValidationError;
use manual_power::validate_manual_power_work;
use prospecting::validate_prospecting_work;
use storage_enclosure_dismantling::validate_storage_enclosure_dismantling_work;
use structural::{validate_structural_construction_work, validate_structural_dismantling_work};
use surface_gathering::validate_surface_gathering_work;
use tree_felling::validate_tree_felling_work;

#[derive(Default)]
struct ActivePlayerJobs {
    manual_production: Option<crate::production::ProductionJobId>,
    mining: Option<crate::mining::MiningJobId>,
    multiple: bool,
}

impl ActivePlayerJobs {
    const fn has_any(&self) -> bool {
        self.manual_production.is_some() || self.mining.is_some()
    }

    const fn has_multiple(&self) -> bool {
        self.multiple
    }

    fn note_manual_production(&mut self, job: crate::production::ProductionJobId) {
        if self.has_any() {
            self.multiple = true;
        }
        self.manual_production.get_or_insert(job);
    }

    fn note_mining(&mut self, job: crate::mining::MiningJobId) {
        if self.has_any() {
            self.multiple = true;
        }
        self.mining.get_or_insert(job);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ActiveWorkSchedule {
    duration: TickSpan,
    remaining: TickSpan,
}

fn project_active_work_schedule(
    current: SimulationTick,
    started_at: SimulationTick,
    completes_at: SimulationTick,
) -> Option<ActiveWorkSchedule> {
    if started_at > current || completes_at <= current {
        return None;
    }
    Some(ActiveWorkSchedule {
        duration: completes_at.checked_duration_since(started_at)?,
        remaining: completes_at.checked_duration_since(current)?,
    })
}

pub(crate) fn validate_loaded_player_work(
    registries: &Registries,
    state: &AppState,
    work_state: &PlayerWorkState,
) -> Result<(), PlayerWorkValidationError> {
    let active_jobs = collect_active_player_jobs(registries, state);
    if active_jobs.has_multiple() {
        return Err(PlayerWorkValidationError::MultiplePlayerJobs);
    }
    let Some(work) = work_state.active() else {
        validate_idle_player_work(&active_jobs)?;
        return validate_direct_consumption_binding(state, None);
    };
    if work_state.revision().checked_add(1).is_none() {
        return Err(PlayerWorkValidationError::RevisionExhausted);
    }
    let player = state
        .survival()
        .player()
        .copied()
        .ok_or(PlayerWorkValidationError::WorkWithoutPlayer)?;
    if player.vitality() == Vitality::ZERO {
        return Err(PlayerWorkValidationError::PlayerDead);
    }
    let locomotion = registries.survival().physiology().locomotion();
    if player_body_uses_generated_world(state, locomotion) {
        if !player_body_has_fixed_floor(registries, state) {
            return Err(PlayerWorkValidationError::PlayerAirborne);
        }
        if state.logistics().player().is_some_and(|player| {
            player.body().horizontal_velocity() != HorizontalVelocity::ZERO
                || player.body().vertical_velocity().nanometers_per_second() != 0
        }) {
            return Err(PlayerWorkValidationError::PlayerMoving);
        }
    }
    let available_energy = player.metabolic_energy();
    let available_hydration = player.hydration();
    match work {
        PlayerWork::TreeFelling { work } => validate_tree_felling_work(
            registries,
            state,
            &active_jobs,
            work,
            available_energy,
            available_hydration,
        ),
        PlayerWork::SurfaceGathering { work } => validate_surface_gathering_work(
            registries,
            state,
            &active_jobs,
            work,
            available_energy,
            available_hydration,
        ),
        PlayerWork::BlockExcavation { work } => validate_block_excavation_work(
            registries,
            state,
            &active_jobs,
            work,
            available_energy,
            available_hydration,
        ),
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
        PlayerWork::EquipmentMaintenance { work } => validate_equipment_maintenance_work(
            registries,
            state,
            &active_jobs,
            work,
            available_energy,
            available_hydration,
        ),
        PlayerWork::StorageEnclosureDismantling { work } => {
            validate_storage_enclosure_dismantling_work(
                registries,
                state,
                &active_jobs,
                work,
                available_energy,
                available_hydration,
            )
        }
        PlayerWork::StructuralConstruction { work } => validate_structural_construction_work(
            registries,
            state,
            &active_jobs,
            work,
            available_energy,
            available_hydration,
        ),
        PlayerWork::StructuralDismantling { work } => validate_structural_dismantling_work(
            registries,
            state,
            &active_jobs,
            work,
            available_energy,
            available_hydration,
        ),
    }?;
    validate_direct_consumption_binding(state, Some(work))
}

fn collect_active_player_jobs(registries: &Registries, state: &AppState) -> ActivePlayerJobs {
    let mut active = ActivePlayerJobs::default();
    for job in state.production().jobs().filter(|job| {
        job.suspension().is_none() && registries.manual_process_exertion(job.process()).is_some()
    }) {
        active.note_manual_production(job.id());
    }
    for job in state.mining().jobs().filter(|job| job.is_working()) {
        active.note_mining(job.id());
    }
    active
}

fn validate_idle_player_work(
    active_jobs: &ActivePlayerJobs,
) -> Result<(), PlayerWorkValidationError> {
    if active_jobs.manual_production.is_some() {
        return Err(PlayerWorkValidationError::ManualProductionMissingWork);
    }
    if active_jobs.mining.is_some() {
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
    if active_jobs.manual_production != Some(job) {
        return Err(PlayerWorkValidationError::ManualProductionMissingWork);
    }
    for stream in record.output_streams() {
        validate_player_stockpile_access(state, stream.destination())
            .map_err(PlayerWorkValidationError::ManualProductionAccess)?;
    }
    let remaining = match record.suspension() {
        Some(suspension) => suspension.remaining_active_time(),
        None => {
            project_active_work_schedule(state.tick(), record.started_at(), record.completes_at())
                .ok_or(PlayerWorkValidationError::ManualProductionScheduleInvalid)?
                .remaining
        }
    };
    validate_remaining_resources(
        registries,
        state,
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
    if active_jobs.mining != Some(job) {
        return Err(PlayerWorkValidationError::MiningMissingWork);
    }
    let method = registries
        .mining()
        .get_method(record.method())
        .ok_or(PlayerWorkValidationError::MiningMethodMissing)?;
    let remaining =
        project_active_work_schedule(state.tick(), record.started_at(), record.completes_at())
            .ok_or(PlayerWorkValidationError::MiningScheduleInvalid)?
            .remaining;
    validate_remaining_resources(
        registries,
        state,
        available_energy,
        available_hydration,
        method.exertion(),
        remaining,
    )
}

pub(super) fn validate_remaining_resources(
    registries: &Registries,
    state: &AppState,
    available_energy: Energy,
    available_hydration: Volume,
    exertion: crate::survival::SurvivalExertion,
    duration: TickSpan,
) -> Result<(), PlayerWorkValidationError> {
    validate_survival_revision_capacity(state, duration)?;
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
    validate_work_reserves_with_pending_locomotion(
        state,
        available_energy,
        available_hydration,
        budget.metabolic_energy(),
        budget.hydration(),
    )
}

/// Trusted loads must preserve the same reserve obligation that work admission
/// establishes. Control-rate walking and jumping can leave exertion unsettled
/// while new work is already active, so the work's remaining cost alone is
/// insufficient to validate the persisted promise.
pub(super) fn validate_work_reserves_with_pending_locomotion(
    state: &AppState,
    available_energy: Energy,
    available_hydration: Volume,
    work_energy: Energy,
    work_hydration: Volume,
) -> Result<(), PlayerWorkValidationError> {
    let pending = state
        .logistics()
        .player()
        .map_or(SurvivalExertion::REST, |player| {
            player.locomotion_exertion().pending()
        });
    let required_energy = work_energy
        .checked_add(pending.energy_cost_per_tick())
        .ok_or(PlayerWorkValidationError::MetabolicCostOverflow)?;
    if available_energy < required_energy {
        return Err(PlayerWorkValidationError::InsufficientMetabolicEnergy {
            available: available_energy,
            required: required_energy,
        });
    }
    let required_hydration = work_hydration
        .checked_add(pending.hydration_loss_per_tick())
        .ok_or(PlayerWorkValidationError::HydrationCostOverflow)?;
    if available_hydration < required_hydration {
        return Err(PlayerWorkValidationError::InsufficientHydration {
            available: available_hydration,
            required: required_hydration,
        });
    }
    Ok(())
}

pub(super) fn validate_survival_revision_capacity(
    state: &AppState,
    duration: TickSpan,
) -> Result<(), PlayerWorkValidationError> {
    if !state.survival().can_advance_revision_by(duration.value()) {
        return Err(PlayerWorkValidationError::SurvivalRevisionExhausted);
    }
    Ok(())
}

#[cfg(test)]
#[path = "validation_tests.rs"]
mod tests;
