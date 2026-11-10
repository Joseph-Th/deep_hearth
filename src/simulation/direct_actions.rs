//! Ordered direct player interactions evaluated after authoritative body motion.

mod contextual;
mod placement;
mod primary;
mod secondary;
mod structural_target;

pub(super) use contextual::attempt_pending_secondary_action;
use contextual::attempt_secondary_action;
pub(super) use primary::attempt_pending_primary_break;
use primary::attempt_primary_action;

use crate::core::state::AppState;
use crate::logistics::{
    PlayerActiveSelection, take_player_pending_primary_break, take_player_pending_secondary_use,
    validate_drop_equipment, validate_select_quick_access_slot,
};
use crate::player::{PlayerActionValidationError, validate_player_action};
use crate::registry::Registries;
use crate::surface::{PlayerMaterialDropOutcome, validate_drop_player_material};

use super::control::{PlayerDirectControlInput, PrimaryControlIntent, SecondaryControlIntent};
use super::outcome::{
    ActiveSelectionDropOutcome, DirectActionOutcomes, PrimaryActionOutcome,
    QuickAccessSelectionOutcome, SecondaryActionOutcome,
};

pub(super) fn attempt_direct_actions(
    registries: &Registries,
    state: &mut AppState,
    input: PlayerDirectControlInput,
    pending_primary: Option<PrimaryActionOutcome>,
    pending_secondary: Option<SecondaryActionOutcome>,
) -> DirectActionOutcomes {
    let quick_access = attempt_quick_access_selection(state, input.quick_access_slot());
    attempt_direct_actions_after_quick_access(
        registries,
        state,
        input,
        quick_access,
        pending_primary,
        pending_secondary,
    )
}

pub(super) fn attempt_direct_actions_after_quick_access(
    registries: &Registries,
    state: &mut AppState,
    input: PlayerDirectControlInput,
    quick_access: Option<QuickAccessSelectionOutcome>,
    pending_primary: Option<PrimaryActionOutcome>,
    pending_secondary: Option<SecondaryActionOutcome>,
) -> DirectActionOutcomes {
    let combat_intent = input.primary().is_some_and(PrimaryControlIntent::is_combat);
    // Only one delayed direct action may own the next slow boundary. A new
    // input on the other button supersedes that reservation as surely as a
    // changed input on the same button. When both buttons are held together,
    // primary retains first admission; neither button steals the other's
    // freshly queued request during this frame.
    if combat_intent
        || input.drop_active_selection()
        || (input.primary().is_none() && input.secondary().is_some())
    {
        let _ = take_player_pending_primary_break(state);
    }
    if input.drop_active_selection()
        || input.primary().is_some()
        || matches!(
            input.secondary(),
            Some(SecondaryControlIntent::PlaceStorage { .. })
        )
    {
        let _ = take_player_pending_secondary_use(state);
    }
    let primary = if pending_primary.is_some() {
        pending_primary
    } else if state.player_work().active().is_none() || combat_intent {
        // Holding excavation through its work interval is silent so it can chain
        // on completion. Combat presses, by contrast, receive the owner's typed
        // rejection instead of vanishing without feedback.
        attempt_primary_action(registries, state, input.primary())
    } else {
        None
    };
    let secondary = if input.drop_active_selection()
        && matches!(
            input.secondary(),
            Some(
                SecondaryControlIntent::UseActive { .. }
                    | SecondaryControlIntent::GatherSurface { .. }
            )
        )
        && validate_player_action(state).is_ok()
    {
        // A held use or explicit gather sampled alongside a drop must not
        // requeue work after the older buffered request was cancelled above.
        pending_secondary
    } else if state.player_work().active().is_some() {
        pending_secondary.or_else(|| match input.secondary() {
            Some(SecondaryControlIntent::PlaceStorage { definition, .. }) => {
                Some(SecondaryActionOutcome::StoragePlacementRejected {
                    definition,
                    error: crate::inventory::PlayerStoragePlacementError::PlayerBusy,
                })
            }
            // A held contextual use is one-shot and stays quiet while work is active.
            Some(
                SecondaryControlIntent::UseActive { .. }
                | SecondaryControlIntent::GatherSurface { .. },
            )
            | None => None,
        })
    } else {
        attempt_secondary_action(registries, state, input.secondary()).or(pending_secondary)
    };
    let drop = attempt_active_selection_drop(registries, state, input.drop_active_selection());
    DirectActionOutcomes::new(quick_access, primary, secondary, drop)
}

pub(super) fn attempt_quick_access_selection(
    state: &mut AppState,
    requested: Option<crate::logistics::PlayerQuickAccessSlot>,
) -> Option<QuickAccessSelectionOutcome> {
    let slot = requested?;
    Some(match validate_select_quick_access_slot(state, slot) {
        Ok(validated) => {
            validated.commit(state).unwrap_or_else(|error| {
                panic!(
                    "runtime invariant broken: synchronous quick-access selection became stale: {error}"
                )
            });
            QuickAccessSelectionOutcome::Selected {
                slot,
                selection: state.logistics().active_selection(),
            }
        }
        Err(error) => QuickAccessSelectionOutcome::Rejected(error),
    })
}

fn attempt_active_selection_drop(
    registries: &Registries,
    state: &mut AppState,
    requested: bool,
) -> Option<ActiveSelectionDropOutcome> {
    if !requested {
        return None;
    }
    match validate_player_action(state) {
        Ok(_) => {}
        Err(PlayerActionValidationError::PlayerNotAdmitted) => {
            return Some(ActiveSelectionDropOutcome::PlayerNotAdmitted);
        }
        Err(PlayerActionValidationError::PlayerDead) => {
            return Some(ActiveSelectionDropOutcome::PlayerDead);
        }
    }
    let Some(selection) = state.logistics().active_selection() else {
        return Some(ActiveSelectionDropOutcome::NoActiveSelection);
    };
    Some(match selection {
        PlayerActiveSelection::Equipment(equipment) => {
            let position = state
                .logistics()
                .player()
                .map(|player| player.position())
                .unwrap_or_else(|| {
                    panic!(
                        "runtime invariant broken: active carried equipment requires an admitted player"
                    )
                });
            match validate_drop_equipment(registries, state, equipment) {
                Ok(validated) => {
                    validated.commit(state).unwrap_or_else(|error| {
                        panic!(
                            "runtime invariant broken: synchronous active-equipment drop became stale: {error}"
                        )
                    });
                    ActiveSelectionDropOutcome::EquipmentDropped {
                        equipment,
                        position,
                    }
                }
                Err(error) => ActiveSelectionDropOutcome::EquipmentRejected(error),
            }
        }
        PlayerActiveSelection::Material(commodity) => {
            match validate_drop_player_material(registries, state, commodity) {
                Ok(validated) => {
                    let outcome: PlayerMaterialDropOutcome =
                        validated.commit(state).unwrap_or_else(|error| {
                            panic!(
                                "runtime invariant broken: synchronous active-material drop became stale: {error}"
                            )
                        });
                    ActiveSelectionDropOutcome::MaterialDropped(outcome)
                }
                Err(error) => ActiveSelectionDropOutcome::MaterialRejected(error),
            }
        }
    })
}

#[cfg(test)]
#[path = "direct_actions_tests.rs"]
mod tests;
