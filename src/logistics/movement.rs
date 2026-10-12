//! Timed face-adjacent open-space movement of the admitted player.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::labor::{
    PlayerStepWork, PlayerWork, PlayerWorkCommitError, PlayerWorkStartError,
    ValidatedPlayerWorkStart, validate_player_work_start,
};
use crate::registry::Registries;
use crate::spatial::VoxelCoord;

use super::assess_player_carrying;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlayerStepError {
    PlayerNotInitialized,
    DestinationNotAdjacent {
        origin: VoxelCoord,
        destination: VoxelCoord,
    },
    DestinationInaccessible {
        origin: VoxelCoord,
        destination: VoxelCoord,
    },
    CompletionTickOverflow,
    LogisticsRevisionExhausted,
    PlayerWork(PlayerWorkStartError),
}

impl Display for PlayerStepError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlayerNotInitialized => {
                formatter.write_str("player movement requires admitted logistics")
            }
            Self::DestinationNotAdjacent {
                origin,
                destination,
            } => write!(
                formatter,
                "player step from ({},{},{}) to ({},{},{}) is not face-adjacent",
                origin.x(),
                origin.y(),
                origin.z(),
                destination.x(),
                destination.y(),
                destination.z()
            ),
            Self::DestinationInaccessible {
                origin,
                destination,
            } => write!(
                formatter,
                "player step from ({},{},{}) to ({},{},{}) has no authorized traversal edge",
                origin.x(),
                origin.y(),
                origin.z(),
                destination.x(),
                destination.y(),
                destination.z()
            ),
            Self::CompletionTickOverflow => {
                formatter.write_str("player step completion tick overflowed")
            }
            Self::LogisticsRevisionExhausted => {
                formatter.write_str("player step cannot reserve its logistics completion revision")
            }
            Self::PlayerWork(error) => {
                write!(formatter, "player step cannot reserve labor: {error}")
            }
        }
    }
}

impl Error for PlayerStepError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::PlayerWork(error) => Some(error),
            Self::PlayerNotInitialized
            | Self::DestinationNotAdjacent { .. }
            | Self::DestinationInaccessible { .. }
            | Self::CompletionTickOverflow
            | Self::LogisticsRevisionExhausted => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlayerStepCommitError {
    StaleLogisticsRevision {
        expected: u64,
        actual: u64,
    },
    PlayerPositionChanged {
        expected: VoxelCoord,
        actual: VoxelCoord,
    },
    PlayerDisappeared,
    PlayerWork(PlayerWorkCommitError),
}

impl Display for PlayerStepCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "player step commit failed: {self:?}")
    }
}

impl Error for PlayerStepCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::PlayerWork(error) => Some(error),
            Self::StaleLogisticsRevision { .. }
            | Self::PlayerPositionChanged { .. }
            | Self::PlayerDisappeared => None,
        }
    }
}

#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub struct ValidatedPlayerStepStart {
    expected_logistics_revision: u64,
    work: PlayerStepWork,
    player_work: ValidatedPlayerWorkStart,
}

impl ValidatedPlayerStepStart {
    #[must_use]
    pub const fn work(&self) -> PlayerStepWork {
        self.work
    }

    pub fn commit(self, state: &mut AppState) -> Result<PlayerStepWork, PlayerStepCommitError> {
        let actual_revision = state.logistics().revision();
        if actual_revision != self.expected_logistics_revision {
            return Err(PlayerStepCommitError::StaleLogisticsRevision {
                expected: self.expected_logistics_revision,
                actual: actual_revision,
            });
        }
        let Some(player) = state.logistics().player() else {
            return Err(PlayerStepCommitError::PlayerDisappeared);
        };
        if player.position() != self.work.origin() {
            return Err(PlayerStepCommitError::PlayerPositionChanged {
                expected: self.work.origin(),
                actual: player.position(),
            });
        }
        self.player_work
            .precheck(state)
            .map_err(PlayerStepCommitError::PlayerWork)?;
        self.player_work.apply(state);
        Ok(self.work)
    }
}

/// Begins one face-adjacent step authorized by persisted world-access state.
///
/// The traversal edge proves only that this immediate move is accessible. It does not provide
/// terrain geometry, collision shape, slope, pathfinding, route selection, or multi-step cost.
pub fn validate_start_player_step(
    registries: &Registries,
    state: &AppState,
    destination: VoxelCoord,
) -> Result<ValidatedPlayerStepStart, PlayerStepError> {
    let carrying = assess_player_carrying(state).ok_or(PlayerStepError::PlayerNotInitialized)?;
    let origin = carrying.position();
    if !origin.is_face_adjacent_to(destination) {
        return Err(PlayerStepError::DestinationNotAdjacent {
            origin,
            destination,
        });
    }
    if !state.logistics().is_step_authorized(origin, destination) {
        return Err(PlayerStepError::DestinationInaccessible {
            origin,
            destination,
        });
    }
    let locomotion = registries.survival().physiology().locomotion();
    let duration = locomotion.step_duration();
    let completes_at = state
        .tick()
        .checked_add_span(duration)
        .ok_or(PlayerStepError::CompletionTickOverflow)?;
    if !state.can_spend_logistics_revisions(1) {
        return Err(PlayerStepError::LogisticsRevisionExhausted);
    }
    let work = PlayerStepWork::new(origin, destination, state.tick(), completes_at);
    let exertion = locomotion.exertion_for_load(carrying.stored(), carrying.capacity());
    let player_work = validate_player_work_start(
        registries,
        state,
        PlayerWork::Step { work },
        duration,
        exertion,
    )
    .map_err(PlayerStepError::PlayerWork)?;
    Ok(ValidatedPlayerStepStart {
        expected_logistics_revision: state.logistics().revision(),
        work,
        player_work,
    })
}

#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerStepOutcome {
    origin: VoxelCoord,
    destination: VoxelCoord,
}

impl PlayerStepOutcome {
    #[must_use]
    pub const fn origin(self) -> VoxelCoord {
        self.origin
    }
    #[must_use]
    pub const fn destination(self) -> VoxelCoord {
        self.destination
    }
}

#[must_use]
pub(crate) struct PlayerStepTickPlan {
    expected_logistics_revision: u64,
    next_logistics_revision: u64,
    work: PlayerStepWork,
}

pub(crate) fn decide_player_step_tick(
    state: &AppState,
    next_tick: SimulationTick,
) -> Result<Option<PlayerStepTickPlan>, ()> {
    let Some(work) = state.player_work().player_step_due_at(next_tick) else {
        return Ok(None);
    };
    let player = state.logistics().player().unwrap_or_else(|| {
        panic!("runtime invariant broken: active player step has no logistics player")
    });
    assert_eq!(
        player.position(),
        work.origin(),
        "runtime invariant broken: active player step origin changed"
    );
    assert!(
        state
            .logistics()
            .is_step_authorized(work.origin(), work.destination()),
        "runtime invariant broken: active player step lost traversal authorization"
    );
    let expected_logistics_revision = state.logistics().revision();
    let next_logistics_revision = expected_logistics_revision.checked_add(1).ok_or(())?;
    Ok(Some(PlayerStepTickPlan {
        expected_logistics_revision,
        next_logistics_revision,
        work,
    }))
}

pub(crate) fn apply_player_step_tick(
    state: &mut AppState,
    plan: Option<PlayerStepTickPlan>,
) -> Option<PlayerStepOutcome> {
    let plan = plan?;
    state.logistics_state_mut().apply_player_step(
        plan.expected_logistics_revision,
        plan.next_logistics_revision,
        plan.work.origin(),
        plan.work.destination(),
    );
    Some(PlayerStepOutcome {
        origin: plan.work.origin(),
        destination: plan.work.destination(),
    })
}

#[cfg(test)]
#[path = "movement_tests.rs"]
mod tests;
