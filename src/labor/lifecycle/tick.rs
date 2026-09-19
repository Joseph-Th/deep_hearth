//! Deterministic player-work release and manual-production resume decisions per tick.

use crate::core::state::AppState;
use crate::core::time::{SimulationTick, TickSpan};
use crate::production::{ProductionAvailabilityChange, find_availability_change};
use crate::registry::Registries;

use super::{PlayerWorkStartError, ValidatedPlayerWorkStart, validate_player_work_start};
use crate::labor::PlayerWork;

mod exertion;

pub(crate) use exertion::player_work_exertion;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PlayerWorkTickError {
    RevisionExhausted,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum PlayerWorkTickPlan {
    Release {
        expected_revision: u64,
        next_revision: u64,
        work: PlayerWork,
    },
    Start(ValidatedPlayerWorkStart),
}

pub(crate) fn decide_manual_production_player_work_start(
    registries: &Registries,
    state: &AppState,
    job: crate::production::ProductionJobId,
    remaining: TickSpan,
) -> Result<Option<ValidatedPlayerWorkStart>, PlayerWorkTickError> {
    let record = state.production().get_job(job).unwrap_or_else(|| {
        panic!(
            "runtime invariant broken: manual production resume references missing production job"
        )
    });
    let exertion = registries
        .manual_process_exertion(record.process())
        .unwrap_or_else(|| {
            panic!(
                "runtime invariant broken: manual production resume references non-manual process"
            )
        });
    match validate_player_work_start(
        registries,
        state,
        PlayerWork::ManualProduction { job },
        remaining,
        exertion,
    ) {
        Ok(start) => Ok(Some(start)),
        Err(PlayerWorkStartError::RevisionExhausted) => Err(PlayerWorkTickError::RevisionExhausted),
        Err(PlayerWorkStartError::MetabolicCostOverflow { .. })
        | Err(PlayerWorkStartError::HydrationCostOverflow { .. }) => {
            panic!(
                "runtime invariant broken: accepted manual production remaining-work budget overflowed"
            )
        }
        Err(PlayerWorkStartError::SurvivalNotInitialized)
        | Err(PlayerWorkStartError::PlayerDead)
        | Err(PlayerWorkStartError::Busy { .. })
        | Err(PlayerWorkStartError::InsufficientMetabolicEnergy { .. })
        | Err(PlayerWorkStartError::InsufficientHydration { .. })
        | Err(PlayerWorkStartError::SurvivalRevisionExhausted { .. }) => Ok(None),
    }
}

fn decide_resumed_manual_production_start(
    registries: &Registries,
    state: &AppState,
    next_tick: SimulationTick,
    production_availability: &[ProductionAvailabilityChange],
) -> Result<Option<PlayerWorkTickPlan>, PlayerWorkTickError> {
    for change in production_availability {
        let ProductionAvailabilityChange::Resumed {
            job,
            scheduled_completion,
            ..
        } = *change
        else {
            continue;
        };
        let record = state.production().get_job(job).unwrap_or_else(|| {
            panic!("runtime invariant broken: resumed production job is missing")
        });
        if registries
            .manual_process_exertion(record.process())
            .is_none()
        {
            continue;
        }
        if scheduled_completion == next_tick {
            return Ok(None);
        }
        let remaining = record
            .suspension()
            .unwrap_or_else(|| {
                panic!("runtime invariant broken: resumed manual production was not suspended")
            })
            .remaining_active_time();
        let start = decide_manual_production_player_work_start(registries, state, job, remaining)?
            .unwrap_or_else(|| {
                panic!(
                    "runtime invariant broken: production resumed manual work without available player labor"
                )
            });
        return Ok(Some(PlayerWorkTickPlan::Start(start)));
    }
    Ok(None)
}

fn manual_production_releases_now(
    state: &AppState,
    job: crate::production::ProductionJobId,
    next_tick: SimulationTick,
    production_availability: &[ProductionAvailabilityChange],
) -> bool {
    let record = state.production().get_job(job).unwrap_or_else(|| {
        panic!("runtime invariant broken: player work references missing manual production job")
    });
    match find_availability_change(production_availability, job) {
        Some(ProductionAvailabilityChange::Suspended { .. }) => true,
        Some(ProductionAvailabilityChange::SuspensionReasonChanged { .. }) => false,
        Some(ProductionAvailabilityChange::Resumed {
            scheduled_completion,
            ..
        }) => scheduled_completion == next_tick,
        None => !record.is_suspended() && record.completes_at() == next_tick,
    }
}

fn active_work_releases_now(
    state: &AppState,
    work: PlayerWork,
    next_tick: SimulationTick,
    production_availability: &[ProductionAvailabilityChange],
    player_dead_after_tick: bool,
) -> bool {
    if player_dead_after_tick {
        return true;
    }
    match work {
        PlayerWork::ManualProduction { job } => {
            manual_production_releases_now(state, job, next_tick, production_availability)
        }
        PlayerWork::Mining { job } => {
            let record = state.mining().get_job(job).unwrap_or_else(|| {
                panic!("runtime invariant broken: player work references missing mining job")
            });
            record.is_working() && record.completes_at() == next_tick
        }
        PlayerWork::ManualPower { work } => work.completes_at() == next_tick,
        PlayerWork::Prospecting { work } => work.completes_at() == next_tick,
        PlayerWork::Eating { work } => work.completes_at() == next_tick,
        PlayerWork::Drinking { work } => work.completes_at() == next_tick,
        PlayerWork::EquipmentMaintenance { work } => work.completes_at() == next_tick,
        PlayerWork::StorageEnclosureDismantling { work } => work.completes_at() == next_tick,
    }
}

pub(crate) fn decide_player_work_tick(
    registries: &Registries,
    state: &AppState,
    next_tick: SimulationTick,
    production_availability: &[ProductionAvailabilityChange],
    player_dead_after_tick: bool,
) -> Result<Option<PlayerWorkTickPlan>, PlayerWorkTickError> {
    let Some(work) = state.player_work().active() else {
        return decide_resumed_manual_production_start(
            registries,
            state,
            next_tick,
            production_availability,
        );
    };
    if !active_work_releases_now(
        state,
        work,
        next_tick,
        production_availability,
        player_dead_after_tick,
    ) {
        return Ok(None);
    }
    let expected_revision = state.player_work().revision();
    let next_revision = expected_revision
        .checked_add(1)
        .ok_or(PlayerWorkTickError::RevisionExhausted)?;
    Ok(Some(PlayerWorkTickPlan::Release {
        expected_revision,
        next_revision,
        work,
    }))
}

pub(crate) fn apply_player_work_tick(state: &mut AppState, plan: Option<PlayerWorkTickPlan>) {
    match plan {
        Some(PlayerWorkTickPlan::Release {
            expected_revision,
            next_revision,
            work,
        }) => {
            state
                .player_work_state_mut()
                .apply_release(expected_revision, next_revision, work);
        }
        Some(PlayerWorkTickPlan::Start(start)) => start.apply(state),
        None => {}
    }
}
