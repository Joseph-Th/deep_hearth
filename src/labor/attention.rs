//! Shared admission and revision binding for actions that require the local player's attention.

use crate::core::state::AppState;
use crate::survival::Vitality;

use super::PlayerWork;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PlayerAttentionError {
    SurvivalNotInitialized,
    PlayerDead,
    Busy { active: PlayerWork },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PlayerAttentionConflict {
    expected: u64,
    actual: u64,
}

impl PlayerAttentionConflict {
    pub(crate) const fn expected(self) -> u64 {
        self.expected
    }

    pub(crate) const fn actual(self) -> u64 {
        self.actual
    }
}

#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ValidatedPlayerAttention {
    expected_revision: u64,
}

impl ValidatedPlayerAttention {
    pub(crate) const fn expected_revision(self) -> u64 {
        self.expected_revision
    }

    pub(crate) fn precheck(self, state: &AppState) -> Result<(), PlayerAttentionConflict> {
        let actual = state.player_work().revision();
        if actual != self.expected_revision {
            return Err(PlayerAttentionConflict {
                expected: self.expected_revision,
                actual,
            });
        }
        Ok(())
    }
}

pub(crate) fn validate_player_attention(
    state: &AppState,
) -> Result<ValidatedPlayerAttention, PlayerAttentionError> {
    let Some(player) = state.survival().player() else {
        return Err(PlayerAttentionError::SurvivalNotInitialized);
    };
    if player.vitality() == Vitality::ZERO {
        return Err(PlayerAttentionError::PlayerDead);
    }
    if let Some(active) = state.player_work().active() {
        return Err(PlayerAttentionError::Busy { active });
    }
    Ok(ValidatedPlayerAttention {
        expected_revision: state.player_work().revision(),
    })
}
