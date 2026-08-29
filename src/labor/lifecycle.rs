//! Revision-bound player-work admission and deterministic release at job completion.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Energy, Volume};
use crate::core::state::AppState;
use crate::core::time::{SimulationTick, TickSpan};
use crate::production::ProductionAvailabilityChange;
use crate::registry::Registries;
use crate::survival::SurvivalExertion;

use super::power_physics::resolve_manual_power_exertion;
use super::{
    PlayerAttentionError, PlayerWork, PlayerWorkResourceBudgetError,
    calculate_player_work_resource_budget, validate_player_attention,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerWorkStartError {
    SurvivalNotInitialized,
    PlayerDead,
    Busy { active: PlayerWork },
    MetabolicCostOverflow { duration: TickSpan },
    InsufficientMetabolicEnergy { available: Energy, required: Energy },
    HydrationCostOverflow { duration: TickSpan },
    InsufficientHydration { available: Volume, required: Volume },
    RevisionExhausted,
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
        PlayerWork::Eating { work: _ } | PlayerWork::Drinking { work: _ } => SurvivalExertion::REST,
    }
}

impl Display for PlayerWorkStartError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SurvivalNotInitialized => {
                formatter.write_str("player work requires initialized survival state")
            }
            Self::PlayerDead => formatter.write_str("dead player cannot begin work"),
            Self::Busy { active } => write!(formatter, "player is already occupied by {active:?}"),
            Self::MetabolicCostOverflow { duration } => write!(
                formatter,
                "player-work metabolic cost overflows across {} active ticks",
                duration.value()
            ),
            Self::InsufficientMetabolicEnergy {
                available,
                required,
            } => write!(
                formatter,
                "player work requires {} nJ of metabolic reserve but only {} nJ is available",
                required.nanojoules(),
                available.nanojoules()
            ),
            Self::HydrationCostOverflow { duration } => write!(
                formatter,
                "player-work hydration cost overflows across {} active ticks",
                duration.value()
            ),
            Self::InsufficientHydration {
                available,
                required,
            } => write!(
                formatter,
                "player work requires {} uL of hydration reserve but only {} uL is available",
                required.microliters(),
                available.microliters()
            ),
            Self::RevisionExhausted => {
                formatter.write_str("player-work revision space is exhausted")
            }
        }
    }
}

impl Error for PlayerWorkStartError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerWorkCommitError {
    StaleRevision { expected: u64, actual: u64 },
    StaleSurvivalRevision { expected: u64, actual: u64 },
}

impl Display for PlayerWorkCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleRevision { expected, actual } => write!(
                formatter,
                "player-work revision changed from {expected} to {actual} after validation"
            ),
            Self::StaleSurvivalRevision { expected, actual } => write!(
                formatter,
                "player-work admission expected survival revision {expected} but current revision is {actual}"
            ),
        }
    }
}

impl Error for PlayerWorkCommitError {}

#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ValidatedPlayerWorkStart {
    expected_revision: u64,
    next_revision: u64,
    expected_survival_revision: u64,
    work: PlayerWork,
    resource_budget: super::PlayerWorkResourceBudget,
}

impl ValidatedPlayerWorkStart {
    pub(crate) const fn resource_budget(&self) -> super::PlayerWorkResourceBudget {
        self.resource_budget
    }

    pub(crate) fn precheck(&self, state: &AppState) -> Result<(), PlayerWorkCommitError> {
        let actual_revision = state.player_work().revision();
        if actual_revision != self.expected_revision {
            return Err(PlayerWorkCommitError::StaleRevision {
                expected: self.expected_revision,
                actual: actual_revision,
            });
        }
        let actual_survival_revision = state.survival().revision();
        if actual_survival_revision != self.expected_survival_revision {
            return Err(PlayerWorkCommitError::StaleSurvivalRevision {
                expected: self.expected_survival_revision,
                actual: actual_survival_revision,
            });
        }
        Ok(())
    }

    pub(crate) fn apply(self, state: &mut AppState) {
        state.player_work_state_mut().apply_start(
            self.expected_revision,
            self.next_revision,
            self.work,
        );
    }
}

pub(crate) fn validate_player_work_start(
    registries: &Registries,
    state: &AppState,
    work: PlayerWork,
    duration: TickSpan,
    exertion: SurvivalExertion,
) -> Result<ValidatedPlayerWorkStart, PlayerWorkStartError> {
    let attention = validate_player_attention(state).map_err(|error| match error {
        PlayerAttentionError::SurvivalNotInitialized => {
            PlayerWorkStartError::SurvivalNotInitialized
        }
        PlayerAttentionError::PlayerDead => PlayerWorkStartError::PlayerDead,
        PlayerAttentionError::Busy { active } => PlayerWorkStartError::Busy { active },
    })?;
    let Some(player) = state.survival().player() else {
        return Err(PlayerWorkStartError::SurvivalNotInitialized);
    };
    let budget = calculate_player_work_resource_budget(
        registries.survival().physiology(),
        exertion,
        duration,
    )
    .map_err(|error| match error {
        PlayerWorkResourceBudgetError::EnergyOverflow => {
            PlayerWorkStartError::MetabolicCostOverflow { duration }
        }
        PlayerWorkResourceBudgetError::HydrationOverflow => {
            PlayerWorkStartError::HydrationCostOverflow { duration }
        }
    })?;
    if player.metabolic_energy() < budget.metabolic_energy() {
        return Err(PlayerWorkStartError::InsufficientMetabolicEnergy {
            available: player.metabolic_energy(),
            required: budget.metabolic_energy(),
        });
    }
    if player.hydration() < budget.hydration() {
        return Err(PlayerWorkStartError::InsufficientHydration {
            available: player.hydration(),
            required: budget.hydration(),
        });
    }
    let expected_revision = attention.expected_revision();
    let next_revision = expected_revision
        .checked_add(1)
        .ok_or(PlayerWorkStartError::RevisionExhausted)?;
    Ok(ValidatedPlayerWorkStart {
        expected_revision,
        next_revision,
        expected_survival_revision: state.survival().revision(),
        work,
        resource_budget: budget,
    })
}

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
                    .is_some_and(|player| player.vitality() == crate::survival::Vitality::ZERO)
        }
        PlayerWork::Drinking { work } => {
            work.completes_at() == next_tick
                || state
                    .survival()
                    .player()
                    .is_some_and(|player| player.vitality() == crate::survival::Vitality::ZERO)
        }
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
