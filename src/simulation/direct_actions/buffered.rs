//! Consistent held-gesture identity and slow-boundary arbitration.

use crate::core::state::AppState;
use crate::logistics::{
    PlayerActiveSelection, take_player_pending_primary_break, take_player_pending_secondary_use,
};
use crate::registry::Registries;
use crate::spatial::{ViewDirection, WorldPoint};

use super::super::control::{
    PlayerDirectControlInput, PrimaryControlIntent, SecondaryControlIntent,
};
use super::super::outcome::{PrimaryActionOutcome, SecondaryActionOutcome};
use super::{contextual::attempt_pending_secondary_action, primary::attempt_pending_primary_break};

pub(super) fn buffered_context_matches(
    state: &AppState,
    position: WorldPoint,
    selection: Option<PlayerActiveSelection>,
) -> bool {
    state.logistics().active_selection() == selection
        && state
            .logistics()
            .player()
            .map(|player| player.body().foot_position())
            == Some(position)
}

pub(super) fn continues_primary(state: &AppState, direction: ViewDirection) -> bool {
    state
        .logistics()
        .pending_primary_break()
        .is_some_and(|pending| {
            pending.direction() == Some(direction)
                && buffered_context_matches(state, pending.position(), pending.selection())
        })
}

pub(super) fn continues_secondary(state: &AppState, direction: ViewDirection) -> bool {
    state
        .logistics()
        .pending_secondary_use()
        .is_some_and(|pending| {
            pending.direction() == Some(direction)
                && buffered_context_matches(
                    state,
                    pending.position(),
                    Some(PlayerActiveSelection::Material(pending.commodity())),
                )
        })
}

/// Snapshot only the kind: a fatal slow tick clears the actual queued record.
#[derive(Clone, Copy)]
pub(in crate::simulation) enum BufferedActionKind {
    Primary,
    Secondary,
}

impl BufferedActionKind {
    pub(in crate::simulation) fn current(state: &AppState) -> Option<Self> {
        if state.logistics().pending_primary_break().is_some() {
            Some(Self::Primary)
        } else if state.logistics().pending_secondary_use().is_some() {
            Some(Self::Secondary)
        } else {
            None
        }
    }

    /// The identical held gesture is not a fresh command just because the
    /// simulation clock reaches its due boundary. A changed hand, origin or
    /// direction may submit new intent; a different command supersedes it.
    pub(in crate::simulation) fn settle(
        self,
        registries: &Registries,
        state: &mut AppState,
        input: PlayerDirectControlInput,
        player_dead: bool,
    ) -> (Option<PrimaryActionOutcome>, Option<SecondaryActionOutcome>) {
        if player_dead {
            if input.primary().is_some()
                || input.secondary().is_some()
                || input.drop_active_selection()
            {
                return (None, None);
            }
            return match self {
                Self::Primary => (Some(PrimaryActionOutcome::PlayerDead), None),
                Self::Secondary => (None, Some(SecondaryActionOutcome::PlayerDead)),
            };
        }
        match self {
            Self::Primary => {
                let replaced = input.drop_active_selection()
                    || match input.primary() {
                        Some(PrimaryControlIntent::BreakTarget { direction }) => {
                            !continues_primary(state, direction)
                        }
                        Some(_) => true,
                        None => input.secondary().is_some(),
                    };
                if replaced {
                    let _ = take_player_pending_primary_break(state);
                    (None, None)
                } else {
                    (attempt_pending_primary_break(registries, state), None)
                }
            }
            Self::Secondary => {
                let replaced = input.drop_active_selection()
                    || input.primary().is_some()
                    || match input.secondary() {
                        Some(SecondaryControlIntent::UseActive { direction }) => {
                            !continues_secondary(state, direction)
                        }
                        Some(_) => true,
                        None => false,
                    };
                if replaced {
                    let _ = take_player_pending_secondary_use(state);
                    (None, None)
                } else {
                    (None, attempt_pending_secondary_action(registries, state))
                }
            }
        }
    }
}
