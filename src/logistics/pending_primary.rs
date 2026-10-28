//! Persistence and mutation boundary for one buffered structural-break input.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::state::AppState;
use crate::player::{
    PlayerActionGuard, PlayerActionValidationError, StalePlayerSurvival, validate_player_action,
};
use crate::spatial::ViewDirection;
use crate::structural::StructuralElementId;

use super::PlayerPendingPrimaryBreak;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerPendingPrimaryBreakError {
    PlayerNotAdmitted,
    PlayerDead,
    AlreadyPending,
    SimulationBoundary,
    LogisticsRevisionExhausted,
}

impl Display for PlayerPendingPrimaryBreakError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlayerNotAdmitted => {
                formatter.write_str("buffered structural break requires an admitted player")
            }
            Self::PlayerDead => formatter.write_str("dead player cannot buffer structural break"),
            Self::AlreadyPending => formatter.write_str(
                "one direct action is already buffered for the next simulation boundary",
            ),
            Self::SimulationBoundary => formatter
                .write_str("structural break does not need buffering on a simulation boundary"),
            Self::LogisticsRevisionExhausted => formatter.write_str(
                "logistics revision space cannot retain and clear buffered structural break",
            ),
        }
    }
}

impl Error for PlayerPendingPrimaryBreakError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PlayerPendingPrimaryBreakCommitError {
    StaleSurvivalRevision { expected: u64, actual: u64 },
    StaleLogisticsRevision { expected: u64, actual: u64 },
}

impl Display for PlayerPendingPrimaryBreakCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleSurvivalRevision { expected, actual } => write!(
                formatter,
                "buffered structural break expected survival revision {expected} but current revision is {actual}"
            ),
            Self::StaleLogisticsRevision { expected, actual } => write!(
                formatter,
                "buffered structural break expected logistics revision {expected} but current revision is {actual}"
            ),
        }
    }
}

impl Error for PlayerPendingPrimaryBreakCommitError {}

#[must_use]
pub(crate) struct ValidatedPlayerPendingPrimaryBreak {
    player_action: PlayerActionGuard,
    expected_revision: u64,
    next_revision: u64,
    pending: PlayerPendingPrimaryBreak,
}

impl ValidatedPlayerPendingPrimaryBreak {
    pub(crate) fn commit(
        self,
        state: &mut AppState,
    ) -> Result<(), PlayerPendingPrimaryBreakCommitError> {
        self.player_action.precheck(state).map_err(
            |StalePlayerSurvival { expected, actual }| {
                PlayerPendingPrimaryBreakCommitError::StaleSurvivalRevision { expected, actual }
            },
        )?;
        let actual = state.logistics().revision();
        if actual != self.expected_revision {
            return Err(
                PlayerPendingPrimaryBreakCommitError::StaleLogisticsRevision {
                    expected: self.expected_revision,
                    actual,
                },
            );
        }
        state.logistics_state_mut().apply_pending_primary_break(
            self.expected_revision,
            self.next_revision,
            self.pending,
        );
        Ok(())
    }
}

pub(crate) fn validate_buffer_player_primary_break(
    state: &AppState,
    element: StructuralElementId,
    direction: ViewDirection,
) -> Result<ValidatedPlayerPendingPrimaryBreak, PlayerPendingPrimaryBreakError> {
    let player_action = validate_player_action(state).map_err(|error| match error {
        PlayerActionValidationError::PlayerNotAdmitted => {
            PlayerPendingPrimaryBreakError::PlayerNotAdmitted
        }
        PlayerActionValidationError::PlayerDead => PlayerPendingPrimaryBreakError::PlayerDead,
    })?;
    let player = state
        .logistics()
        .player()
        .copied()
        .unwrap_or_else(|| unreachable!("validated player action requires logistics player"));
    if state.logistics().has_pending_direct_action() {
        return Err(PlayerPendingPrimaryBreakError::AlreadyPending);
    }
    if state.control_tick().is_simulation_boundary() {
        return Err(PlayerPendingPrimaryBreakError::SimulationBoundary);
    }
    if !state.can_spend_logistics_revisions(2) {
        return Err(PlayerPendingPrimaryBreakError::LogisticsRevisionExhausted);
    }
    let expected_revision = state.logistics().revision();
    let next_revision = expected_revision
        .checked_add(1)
        .unwrap_or_else(|| unreachable!("two-step logistics headroom was prevalidated"));
    Ok(ValidatedPlayerPendingPrimaryBreak {
        player_action,
        expected_revision,
        next_revision,
        pending: PlayerPendingPrimaryBreak::new(
            state.control_tick(),
            player.body().foot_position(),
            element,
            direction,
        ),
    })
}

pub(crate) fn take_player_pending_primary_break(
    state: &mut AppState,
) -> Option<PlayerPendingPrimaryBreak> {
    let pending = state.logistics().pending_primary_break()?;
    let expected_revision = state.logistics().revision();
    let next_revision = expected_revision.checked_add(1).unwrap_or_else(|| {
        panic!("runtime invariant broken: buffered structural break lost reserved clear revision")
    });
    state
        .logistics_state_mut()
        .apply_clear_pending_primary_break(expected_revision, next_revision, pending);
    Some(pending)
}
