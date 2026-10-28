//! Persistence and mutation boundary for one buffered contextual-use input.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::player::{
    PlayerActionGuard, PlayerActionValidationError, StalePlayerSurvival, validate_player_action,
};
use crate::spatial::ViewDirection;
use crate::surface::SurfaceResourceId;

use super::PlayerPendingSecondaryUse;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerPendingSecondaryUseError {
    PlayerNotAdmitted,
    PlayerDead,
    AlreadyPending,
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
pub(crate) enum PlayerPendingSecondaryUseCommitError {
    StaleSurvivalRevision { expected: u64, actual: u64 },
    StaleLogisticsRevision { expected: u64, actual: u64 },
}

impl Display for PlayerPendingSecondaryUseCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleSurvivalRevision { expected, actual } => write!(
                formatter,
                "buffered contextual use expected survival revision {expected} but current revision is {actual}"
            ),
            Self::StaleLogisticsRevision { expected, actual } => write!(
                formatter,
                "buffered contextual use expected logistics revision {expected} but current revision is {actual}"
            ),
        }
    }
}

impl Error for PlayerPendingSecondaryUseCommitError {}

#[must_use]
pub(crate) struct ValidatedPlayerPendingSecondaryUse {
    player_action: PlayerActionGuard,
    expected_revision: u64,
    next_revision: u64,
    pending: PlayerPendingSecondaryUse,
}

impl ValidatedPlayerPendingSecondaryUse {
    pub(crate) fn commit(
        self,
        state: &mut AppState,
    ) -> Result<(), PlayerPendingSecondaryUseCommitError> {
        self.player_action.precheck(state).map_err(
            |StalePlayerSurvival { expected, actual }| {
                PlayerPendingSecondaryUseCommitError::StaleSurvivalRevision { expected, actual }
            },
        )?;
        let actual = state.logistics().revision();
        if actual != self.expected_revision {
            return Err(
                PlayerPendingSecondaryUseCommitError::StaleLogisticsRevision {
                    expected: self.expected_revision,
                    actual,
                },
            );
        }
        state.logistics_state_mut().apply_pending_secondary_use(
            self.expected_revision,
            self.next_revision,
            self.pending,
        );
        Ok(())
    }
}

pub(crate) fn validate_buffer_player_secondary_use(
    state: &AppState,
    direction: ViewDirection,
) -> Result<ValidatedPlayerPendingSecondaryUse, PlayerPendingSecondaryUseError> {
    validate_buffer_player_secondary_request(state, direction, None)
}

pub(crate) fn validate_buffer_player_secondary_surface_gathering(
    state: &AppState,
    direction: ViewDirection,
    resource: SurfaceResourceId,
    mass: Mass,
) -> Result<ValidatedPlayerPendingSecondaryUse, PlayerPendingSecondaryUseError> {
    validate_buffer_player_secondary_request(state, direction, Some((resource, mass)))
}

fn validate_buffer_player_secondary_request(
    state: &AppState,
    direction: ViewDirection,
    surface_request: Option<(SurfaceResourceId, Mass)>,
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
    // One revision records the buffered input and one remains reserved for clearing it. The
    // latter becomes resident future demand after commit.
    if !state.can_spend_logistics_revisions(2) {
        return Err(PlayerPendingSecondaryUseError::LogisticsRevisionExhausted);
    }
    let expected_revision = state.logistics().revision();
    let next_revision = expected_revision
        .checked_add(1)
        .unwrap_or_else(|| unreachable!("two-step logistics headroom was prevalidated"));
    Ok(ValidatedPlayerPendingSecondaryUse {
        player_action,
        expected_revision,
        next_revision,
        pending: PlayerPendingSecondaryUse::new(
            state.control_tick(),
            player.body().foot_position(),
            state.logistics().active_selection(),
            direction,
        )
        .with_surface_request(surface_request),
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
