//! Exhaustive persistence validation for cross-owner player labor references.

use crate::core::quantity::{Energy, Volume};
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::registry::Registries;
use crate::survival::Vitality;

use super::{
    PlayerWork, PlayerWorkResourceBudgetError, PlayerWorkState,
    calculate_player_work_resource_budget,
};

mod direct_consumption;
mod error;
mod manual_power;
mod prospecting;

use direct_consumption::{
    validate_direct_consumption_binding, validate_drinking_work, validate_eating_work,
};
pub use error::PlayerWorkValidationError;
use manual_power::validate_manual_power_work;
use prospecting::validate_prospecting_work;

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
        validate_idle_player_work(&active_jobs)?;
        return validate_direct_consumption_binding(state, None);
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
    }?;
    validate_direct_consumption_binding(state, Some(work))
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

pub(super) fn validate_remaining_resources(
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
