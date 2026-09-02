//! Deterministic player-work exertion, release, and manual-production resume decisions per tick.

use crate::core::state::AppState;
use crate::core::time::{SimulationTick, TickSpan};
use crate::production::ProductionAvailabilityChange;
use crate::registry::Registries;
use crate::survival::{SurvivalExertion, Vitality};

use super::{PlayerWorkStartError, ValidatedPlayerWorkStart, validate_player_work_start};
use crate::labor::PlayerWork;
use crate::labor::power_physics::resolve_manual_power_exertion;

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

/// Resolves the incremental physiological cost of the currently active player-owned job.
#[must_use]
pub(crate) fn player_work_exertion(
    registries: &Registries,
    state: &AppState,
    production_availability: &[ProductionAvailabilityChange],
) -> SurvivalExertion {
    let Some(work) = state.player_work().active() else {
        let mut resumed_manual_work = production_availability.iter().filter_map(|change| {
            let ProductionAvailabilityChange::Resumed { job, .. } = *change else {
                return None;
            };
            let record = state.production().get_job(job).unwrap_or_else(|| {
                panic!("runtime invariant broken: resumed production job is missing")
            });
            registries.manual_process_exertion(record.process())
        });
        let exertion = resumed_manual_work.next().unwrap_or(SurvivalExertion::REST);
        assert!(
            resumed_manual_work.next().is_none(),
            "runtime invariant broken: more than one manual production job resumed in one tick"
        );
        return exertion;
    };
    match work {
        PlayerWork::ManualProduction { job } => {
            let record = state.production().get_job(job).unwrap_or_else(|| {
                panic!(
                    "runtime invariant broken: player work references missing manual production job"
                )
            });
            let active_this_tick = production_availability
                .iter()
                .copied()
                .find(|change| change.job() == job)
                .map_or(!record.is_suspended(), |change| {
                    matches!(change, ProductionAvailabilityChange::Resumed { .. })
                });
            if !active_this_tick {
                return SurvivalExertion::REST;
            }
            registries
                .manual_process_exertion(record.process())
                .unwrap_or_else(|| {
                    panic!(
                        "runtime invariant broken: player production job has no manual definition"
                    )
                })
        }
        PlayerWork::Mining { job } => {
            let record = state.mining().get_job(job).unwrap_or_else(|| {
                panic!("runtime invariant broken: player work references missing mining job")
            });
            registries
                .mining()
                .get_method(record.method())
                .unwrap_or_else(|| {
                    panic!("runtime invariant broken: player mining job has no method definition")
                })
                .exertion()
        }
        PlayerWork::ManualPower { work } => {
            let definition = registries
                .labor()
                .get_manual_power(work.method())
                .copied()
                .unwrap_or_else(|| {
                    panic!("runtime invariant broken: player power work has no method definition")
                });
            let duration = TickSpan::new(work.completes_at().value() - work.started_at().value());
            resolve_manual_power_exertion(
                work.output().energy(),
                duration,
                definition.maximum_exertion(),
                definition.metabolic_efficiency_ppm(),
            )
            .unwrap_or_else(|error| {
                panic!("runtime invariant broken: manual power exertion is invalid: {error:?}")
            })
        }
        PlayerWork::Prospecting { work } => registries
            .labor()
            .get_prospecting(work.method())
            .unwrap_or_else(|| {
                panic!("runtime invariant broken: player prospecting work has no method definition")
            })
            .exertion(),
        PlayerWork::EquipmentMaintenance { work } => {
            let record = state
                .equipment()
                .get_equipment(work.equipment())
                .unwrap_or_else(|| {
                    panic!(
                        "runtime invariant broken: maintenance work references missing equipment"
                    )
                });
            registries
                .equipment()
                .get_equipment(record.definition())
                .and_then(|definition| definition.maintenance_profile())
                .unwrap_or_else(|| {
                    panic!(
                        "runtime invariant broken: maintenance work has no authored service profile"
                    )
                })
                .exertion()
        }
        PlayerWork::StorageEnclosureDismantling { work } => registries
            .storage()
            .get(work.definition())
            .unwrap_or_else(|| {
                panic!("runtime invariant broken: storage dismantling has no authored definition")
            })
            .dismantle_exertion(),
        PlayerWork::Eating { work: _ } | PlayerWork::Drinking { work: _ } => SurvivalExertion::REST,
    }
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
        | Err(PlayerWorkStartError::InsufficientHydration { .. }) => Ok(None),
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
    match production_availability
        .iter()
        .copied()
        .find(|change| change.job() == job)
    {
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
) -> bool {
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
        PlayerWork::Eating { work } => {
            work.completes_at() == next_tick
                || state
                    .survival()
                    .player()
                    .is_some_and(|player| player.vitality() == Vitality::ZERO)
        }
        PlayerWork::Drinking { work } => {
            work.completes_at() == next_tick
                || state
                    .survival()
                    .player()
                    .is_some_and(|player| player.vitality() == Vitality::ZERO)
        }
        PlayerWork::EquipmentMaintenance { work } => work.completes_at() == next_tick,
        PlayerWork::StorageEnclosureDismantling { work } => work.completes_at() == next_tick,
    }
}

pub(crate) fn decide_player_work_tick(
    registries: &Registries,
    state: &AppState,
    next_tick: SimulationTick,
    production_availability: &[ProductionAvailabilityChange],
) -> Result<Option<PlayerWorkTickPlan>, PlayerWorkTickError> {
    let Some(work) = state.player_work().active() else {
        return decide_resumed_manual_production_start(
            registries,
            state,
            next_tick,
            production_availability,
        );
    };
    if !active_work_releases_now(state, work, next_tick, production_availability) {
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
