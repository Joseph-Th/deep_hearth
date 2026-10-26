//! Shared admission and revision binding for actions that require the local player's attention.

use crate::core::state::AppState;
use crate::survival::Vitality;

use super::PlayerWork;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PlayerAttentionError {
    SurvivalNotInitialized,
    PlayerDead,
    Busy { active: Box<PlayerWork> },
    PreparedStrikeActive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PlayerAttentionConflict {
    WorkRevision { expected: u64, actual: u64 },
    CombatRevision { expected: u64, actual: u64 },
}

#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ValidatedPlayerAttention {
    expected_revision: u64,
    expected_combat_revision: u64,
}

#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ValidatedPlayerAttentionHold {
    expected_revision: u64,
    expected_combat_revision: u64,
    next_revision: u64,
    work: PlayerWork,
}

impl ValidatedPlayerAttention {
    pub(crate) fn hold(self, work: PlayerWork) -> Option<ValidatedPlayerAttentionHold> {
        // A delayed attention hold mutates player-work once when admitted and once when the
        // interval releases. Reserve both revisions before the first mutation.
        self.expected_revision.checked_add(2)?;
        let next_revision = self.expected_revision.checked_add(1).unwrap_or_else(|| {
            unreachable!("two-step attention revision budget includes admission")
        });
        Some(ValidatedPlayerAttentionHold {
            expected_revision: self.expected_revision,
            expected_combat_revision: self.expected_combat_revision,
            next_revision,
            work,
        })
    }
}

impl ValidatedPlayerAttentionHold {
    pub(crate) fn precheck(&self, state: &AppState) -> Result<(), PlayerAttentionConflict> {
        let actual = state.player_work().revision();
        if actual != self.expected_revision {
            return Err(PlayerAttentionConflict::WorkRevision {
                expected: self.expected_revision,
                actual,
            });
        }
        let actual = state.combat().revision();
        if actual != self.expected_combat_revision {
            return Err(PlayerAttentionConflict::CombatRevision {
                expected: self.expected_combat_revision,
                actual,
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
        return Err(PlayerAttentionError::Busy {
            active: Box::new(active),
        });
    }
    // A raised weapon is already an exclusive actor commitment. All consumers
    // of attention (work, meals, drinks) must honor the same admission rule.
    if state.combat().has_prepared_strike() {
        return Err(PlayerAttentionError::PreparedStrikeActive);
    }
    Ok(ValidatedPlayerAttention {
        expected_revision: state.player_work().revision(),
        expected_combat_revision: state.combat().revision(),
    })
}
