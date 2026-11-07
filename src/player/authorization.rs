//! Liveness- and revision-bound authorization for direct player-owned actions.

use crate::core::state::AppState;
use crate::survival::Vitality;

use super::{PlayerAdmissionState, player_admission_state};

/// Survival-snapshot authorization proof for direct player-owned manipulation.
///
/// Owner APIs also serve controlled capability setup before ordinary player admission. Once a
/// player is fully admitted, however, the same operation is player-owned and terminal vitality
/// must block it. The proof captures survival revision so an authorization granted to a living
/// player cannot commit after survival changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PlayerActionGuard {
    expected_survival_revision: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PlayerActionValidationError {
    PlayerNotAdmitted,
    PlayerDead,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PlayerDeadActionError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StalePlayerSurvival {
    pub(crate) expected: u64,
    pub(crate) actual: u64,
}

impl PlayerActionGuard {
    pub(crate) fn precheck(self, state: &AppState) -> Result<(), StalePlayerSurvival> {
        let Some(expected) = self.expected_survival_revision else {
            return Ok(());
        };
        let actual = state.survival().revision();
        if actual != expected {
            return Err(StalePlayerSurvival { expected, actual });
        }
        Ok(())
    }
}

pub(crate) fn validate_optional_player_action(
    state: &AppState,
) -> Result<PlayerActionGuard, PlayerDeadActionError> {
    if player_admission_state(state) != PlayerAdmissionState::Admitted {
        return Ok(PlayerActionGuard {
            expected_survival_revision: None,
        });
    }
    let player = state
        .survival()
        .player()
        .unwrap_or_else(|| unreachable!("admitted player has survival state"));
    if player.vitality() == Vitality::ZERO {
        return Err(PlayerDeadActionError);
    }
    Ok(PlayerActionGuard {
        expected_survival_revision: Some(state.survival().revision()),
    })
}

pub(crate) fn validate_player_action(
    state: &AppState,
) -> Result<PlayerActionGuard, PlayerActionValidationError> {
    if player_admission_state(state) != PlayerAdmissionState::Admitted {
        return Err(PlayerActionValidationError::PlayerNotAdmitted);
    }
    let player = state
        .survival()
        .player()
        .unwrap_or_else(|| unreachable!("admitted player has survival state"));
    if player.vitality() == Vitality::ZERO {
        return Err(PlayerActionValidationError::PlayerDead);
    }
    Ok(PlayerActionGuard {
        expected_survival_revision: Some(state.survival().revision()),
    })
}
