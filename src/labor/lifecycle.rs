//! Revision-bound player-work admission and deterministic release at job completion.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::registry::Registries;
use crate::survival::{SurvivalExertion, Vitality};

use super::PlayerWork;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerWorkStartError {
    SurvivalNotInitialized,
    PlayerDead,
    Busy { active: PlayerWork },
    RevisionExhausted,
}

/// Resolves the incremental physiological cost of the currently active player-owned job.
#[must_use]
pub(crate) fn player_work_exertion(registries: &Registries, state: &AppState) -> SurvivalExertion {
    let Some(work) = state.player_work().active() else {
        return SurvivalExertion::REST;
    };
    match work {
        PlayerWork::ManualCraft { job } => {
            let record = state.production().get_job(job).unwrap_or_else(|| {
                panic!("runtime invariant broken: player work references missing craft job")
            });
            registries
                .crafting()
                .get_manual(record.process())
                .unwrap_or_else(|| {
                    panic!("runtime invariant broken: player craft job has no manual definition")
                })
                .exertion()
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
        PlayerWork::ManualPower { work } => registries
            .labor()
            .get_manual_power(work.method())
            .unwrap_or_else(|| {
                panic!("runtime invariant broken: player power work has no method definition")
            })
            .exertion(),
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
}

impl Display for PlayerWorkCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleRevision { expected, actual } => write!(
                formatter,
                "player-work revision changed from {expected} to {actual} after validation"
            ),
        }
    }
}

impl Error for PlayerWorkCommitError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ValidatedPlayerWorkStart {
    expected_revision: u64,
    next_revision: u64,
    work: PlayerWork,
}

impl ValidatedPlayerWorkStart {
    pub(crate) fn precheck(self, state: &AppState) -> Result<(), PlayerWorkCommitError> {
        let actual_revision = state.player_work().revision();
        if actual_revision != self.expected_revision {
            return Err(PlayerWorkCommitError::StaleRevision {
                expected: self.expected_revision,
                actual: actual_revision,
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
    state: &AppState,
    work: PlayerWork,
) -> Result<ValidatedPlayerWorkStart, PlayerWorkStartError> {
    let Some(player) = state.survival().player() else {
        return Err(PlayerWorkStartError::SurvivalNotInitialized);
    };
    if player.vitality() == Vitality::ZERO {
        return Err(PlayerWorkStartError::PlayerDead);
    }
    if let Some(active) = state.player_work().active() {
        return Err(PlayerWorkStartError::Busy { active });
    }
    let expected_revision = state.player_work().revision();
    let next_revision = expected_revision
        .checked_add(1)
        .ok_or(PlayerWorkStartError::RevisionExhausted)?;
    Ok(ValidatedPlayerWorkStart {
        expected_revision,
        next_revision,
        work,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PlayerWorkTickError {
    RevisionExhausted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PlayerWorkTickPlan {
    expected_revision: u64,
    next_revision: u64,
    work: PlayerWork,
}

pub(crate) fn decide_player_work_tick(
    state: &AppState,
    next_tick: SimulationTick,
) -> Result<Option<PlayerWorkTickPlan>, PlayerWorkTickError> {
    let Some(work) = state.player_work().active() else {
        return Ok(None);
    };
    let releases_now = match work {
        PlayerWork::ManualCraft { job } => {
            let record = state.production().get_job(job).unwrap_or_else(|| {
                panic!("runtime invariant broken: player work references missing craft job")
            });
            !record.is_suspended() && record.completes_at() == next_tick
        }
        PlayerWork::Mining { job } => {
            let record = state.mining().get_job(job).unwrap_or_else(|| {
                panic!("runtime invariant broken: player work references missing mining job")
            });
            record.is_working() && record.completes_at() == next_tick
        }
        PlayerWork::ManualPower { work } => work.completes_at() == next_tick,
    };
    if !releases_now {
        return Ok(None);
    }
    let expected_revision = state.player_work().revision();
    let next_revision = expected_revision
        .checked_add(1)
        .ok_or(PlayerWorkTickError::RevisionExhausted)?;
    Ok(Some(PlayerWorkTickPlan {
        expected_revision,
        next_revision,
        work,
    }))
}

pub(crate) fn apply_player_work_tick(state: &mut AppState, plan: Option<PlayerWorkTickPlan>) {
    if let Some(plan) = plan {
        state.player_work_state_mut().apply_release(
            plan.expected_revision,
            plan.next_revision,
            plan.work,
        );
    }
}
