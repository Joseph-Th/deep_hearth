//! Persistence and mutation boundary for buffered primary and secondary direct inputs.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::state::AppState;
use crate::core::time::ControlTick;
use crate::player::{
    PlayerActionGuard, PlayerActionValidationError, StalePlayerSurvival, validate_player_action,
};
use crate::spatial::ViewDirection;
use crate::structural::StructuralElementId;

use super::{
    BufferedSecondaryTarget, PlayerActiveSelection, PlayerPendingPrimaryBreak,
    PlayerPendingSecondaryUse,
};

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
pub enum PlayerPendingSecondaryUseError {
    PlayerNotAdmitted,
    PlayerDead,
    AlreadyPending,
    MaterialSelectionRequired,
    SimulationBoundary,
    LogisticsRevisionExhausted,
}

impl Display for PlayerPendingSecondaryUseError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlayerNotAdmitted => {
                formatter.write_str("buffered contextual use requires an admitted player")
            }
            Self::PlayerDead => formatter.write_str("dead player cannot buffer contextual use"),
            Self::AlreadyPending => formatter.write_str(
                "one contextual use is already buffered for the next simulation boundary",
            ),
            Self::MaterialSelectionRequired => {
                formatter.write_str("buffered contextual use requires a selected material")
            }
            Self::SimulationBoundary => formatter
                .write_str("contextual use does not need buffering on a simulation boundary"),
            Self::LogisticsRevisionExhausted => formatter.write_str(
                "logistics revision space cannot retain and clear buffered contextual use",
            ),
        }
    }
}

impl Error for PlayerPendingSecondaryUseError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PendingDirectActionCommitError {
    ControlTick {
        expected: ControlTick,
        actual: ControlTick,
    },
    SurvivalRevision {
        expected: u64,
        actual: u64,
    },
    LogisticsRevision {
        expected: u64,
        actual: u64,
    },
}

impl Display for PendingDirectActionCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ControlTick { expected, actual } => write!(
                formatter,
                "buffered direct action expected control tick {} but current control tick is {}",
                expected.value(),
                actual.value()
            ),
            Self::SurvivalRevision { expected, actual } => write!(
                formatter,
                "buffered direct action expected survival revision {expected} but current revision is {actual}"
            ),
            Self::LogisticsRevision { expected, actual } => write!(
                formatter,
                "buffered direct action expected logistics revision {expected} but current revision is {actual}"
            ),
        }
    }
}

impl Error for PendingDirectActionCommitError {}

/// Shared transactional precheck for both kinds of buffered direct action.
struct BufferedActionGuard {
    player_action: PlayerActionGuard,
    expected_revision: u64,
    next_revision: u64,
}

impl BufferedActionGuard {
    fn new(state: &AppState, player_action: PlayerActionGuard) -> Self {
        let expected_revision = state.logistics().revision();
        let next_revision = expected_revision
            .checked_add(1)
            .expect("two-step logistics headroom was prevalidated");
        Self {
            player_action,
            expected_revision,
            next_revision,
        }
    }

    fn precheck(
        &self,
        state: &AppState,
        expected: ControlTick,
    ) -> Result<(), PendingDirectActionCommitError> {
        let actual = state.control_tick();
        if expected != actual {
            return Err(PendingDirectActionCommitError::ControlTick { expected, actual });
        }
        self.player_action.precheck(state).map_err(
            |StalePlayerSurvival { expected, actual }| {
                PendingDirectActionCommitError::SurvivalRevision { expected, actual }
            },
        )?;
        let actual = state.logistics().revision();
        if actual != self.expected_revision {
            return Err(PendingDirectActionCommitError::LogisticsRevision {
                expected: self.expected_revision,
                actual,
            });
        }
        Ok(())
    }
}

#[must_use]
pub(crate) struct ValidatedPlayerPendingPrimaryBreak {
    guard: BufferedActionGuard,
    pending: PlayerPendingPrimaryBreak,
}

impl ValidatedPlayerPendingPrimaryBreak {
    pub(crate) fn commit(self, state: &mut AppState) -> Result<(), PendingDirectActionCommitError> {
        self.guard.precheck(state, self.pending.requested_at())?;
        state.logistics_state_mut().apply_pending_primary_break(
            self.guard.expected_revision,
            self.guard.next_revision,
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
    Ok(ValidatedPlayerPendingPrimaryBreak {
        guard: BufferedActionGuard::new(state, player_action),
        pending: PlayerPendingPrimaryBreak::new(
            state.control_tick(),
            player.body().foot_position(),
            element,
            state.logistics().active_selection(),
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

#[must_use]
pub(crate) struct ValidatedPlayerPendingSecondaryUse {
    guard: BufferedActionGuard,
    pending: PlayerPendingSecondaryUse,
}

impl ValidatedPlayerPendingSecondaryUse {
    pub(crate) fn commit(self, state: &mut AppState) -> Result<(), PendingDirectActionCommitError> {
        self.guard.precheck(state, self.pending.requested_at())?;
        state.logistics_state_mut().apply_pending_secondary_use(
            self.guard.expected_revision,
            self.guard.next_revision,
            self.pending,
        );
        Ok(())
    }
}

pub(crate) fn validate_buffer_player_secondary_use(
    state: &AppState,
    direction: ViewDirection,
    target: BufferedSecondaryTarget,
) -> Result<ValidatedPlayerPendingSecondaryUse, PlayerPendingSecondaryUseError> {
    let player_action = validate_player_action(state).map_err(|error| match error {
        PlayerActionValidationError::PlayerNotAdmitted => {
            PlayerPendingSecondaryUseError::PlayerNotAdmitted
        }
        PlayerActionValidationError::PlayerDead => PlayerPendingSecondaryUseError::PlayerDead,
    })?;
    let player = state
        .logistics()
        .player()
        .copied()
        .unwrap_or_else(|| unreachable!("validated player action requires logistics player"));
    if state.logistics().has_pending_direct_action() {
        return Err(PlayerPendingSecondaryUseError::AlreadyPending);
    }
    if state.control_tick().is_simulation_boundary() {
        return Err(PlayerPendingSecondaryUseError::SimulationBoundary);
    }
    let Some(PlayerActiveSelection::Material(commodity)) = state.logistics().active_selection()
    else {
        return Err(PlayerPendingSecondaryUseError::MaterialSelectionRequired);
    };
    // One revision records the buffered input and one remains reserved for clearing it. The
    // latter becomes resident future demand after commit.
    if !state.can_spend_logistics_revisions(2) {
        return Err(PlayerPendingSecondaryUseError::LogisticsRevisionExhausted);
    }
    Ok(ValidatedPlayerPendingSecondaryUse {
        guard: BufferedActionGuard::new(state, player_action),
        pending: PlayerPendingSecondaryUse::new(
            state.control_tick(),
            player.body().foot_position(),
            commodity,
            direction,
            target,
        ),
    })
}

pub(crate) fn take_player_pending_secondary_use(
    state: &mut AppState,
) -> Option<PlayerPendingSecondaryUse> {
    let pending = state.logistics().pending_secondary_use()?;
    let expected_revision = state.logistics().revision();
    let next_revision = expected_revision.checked_add(1).unwrap_or_else(|| {
        panic!("runtime invariant broken: buffered contextual use lost reserved clear revision")
    });
    state
        .logistics_state_mut()
        .apply_clear_pending_secondary_use(expected_revision, next_revision, pending);
    Some(pending)
}
