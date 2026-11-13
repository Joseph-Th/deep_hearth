//! Contextual use, gathering and storage placement admission at the direct-control boundary.

use crate::core::state::AppState;
use crate::logistics::{take_player_pending_secondary_use, validate_buffer_player_secondary_use};
use crate::player::{PlayerActionValidationError, validate_player_action};
use crate::registry::Registries;
use crate::spatial::ViewDirection;

use super::super::control::SecondaryControlIntent;
use super::super::outcome::SecondaryActionOutcome;
use super::placement::attempt_storage_placement;
use super::secondary::{
    attempt_empty_hand_secondary_action, attempt_explicit_surface_gathering,
    attempt_selected_drinking, attempt_selected_eating, attempt_selected_secondary_action,
    attempt_selected_surface_gathering,
};

pub(super) fn attempt_secondary_action(
    registries: &Registries,
    state: &mut AppState,
    intent: Option<SecondaryControlIntent>,
) -> Option<SecondaryActionOutcome> {
    let intent = intent?;
    if let Some(pending) = state.logistics().pending_secondary_use() {
        if matches!(
            intent,
            SecondaryControlIntent::UseActive { direction }
                if pending.direction() == Some(direction)
                    && pending.selection() == state.logistics().active_selection()
                    && state.logistics().player().map(|player| player.body().foot_position())
                        == Some(pending.position())
        ) {
            return Some(SecondaryActionOutcome::QueuedUntilSimulationBoundary);
        }
        // A fresh contextual selection, aim or actor position replaces the earlier
        // queued use; otherwise the old gesture fires or fails on the slow boundary.
        let _ = take_player_pending_secondary_use(state);
    }
    let direction = match intent {
        SecondaryControlIntent::UseActive { direction } => direction,
        SecondaryControlIntent::GatherSurface { resource, mass } => {
            return Some(attempt_explicit_surface_gathering(
                registries, state, resource, mass,
            ));
        }
        SecondaryControlIntent::GatherSurfaceOption { option, mass } => {
            return Some(attempt_selected_surface_gathering(
                registries, state, option, mass,
            ));
        }
        SecondaryControlIntent::EatStack { choice } => {
            return Some(attempt_selected_eating(registries, state, choice));
        }
        SecondaryControlIntent::DrinkSource { store } => {
            return Some(attempt_selected_drinking(registries, state, store));
        }
        SecondaryControlIntent::PlaceStorage {
            definition,
            direction,
        } => {
            return Some(match validate_player_action(state) {
                Ok(_) => attempt_storage_placement(registries, state, definition, direction),
                Err(PlayerActionValidationError::PlayerNotAdmitted) => {
                    SecondaryActionOutcome::PlayerNotAdmitted
                }
                Err(PlayerActionValidationError::PlayerDead) => SecondaryActionOutcome::PlayerDead,
            });
        }
    };
    let outcome = attempt_secondary_action_now(registries, state, direction);
    if outcome != SecondaryActionOutcome::DeferredUntilSimulationBoundary {
        return Some(outcome);
    }
    let buffered = validate_buffer_player_secondary_use(state, direction);
    Some(match buffered {
        Ok(validated) => {
            validated.commit(state).unwrap_or_else(|error| {
                panic!(
                    "runtime invariant broken: synchronous contextual-use buffering became stale: {error}"
                )
            });
            SecondaryActionOutcome::QueuedUntilSimulationBoundary
        }
        Err(error) => SecondaryActionOutcome::BufferRejected(error),
    })
}

fn attempt_secondary_action_now(
    registries: &Registries,
    state: &mut AppState,
    direction: ViewDirection,
) -> SecondaryActionOutcome {
    match validate_player_action(state) {
        Ok(_) => {}
        Err(PlayerActionValidationError::PlayerNotAdmitted) => {
            return SecondaryActionOutcome::PlayerNotAdmitted;
        }
        Err(PlayerActionValidationError::PlayerDead) => {
            return SecondaryActionOutcome::PlayerDead;
        }
    }
    match state.logistics().active_selection() {
        None => attempt_empty_hand_secondary_action(registries, state),
        Some(selection) => {
            attempt_selected_secondary_action(registries, state, selection, direction)
        }
    }
}

pub(in crate::simulation) fn attempt_pending_secondary_action(
    registries: &Registries,
    state: &mut AppState,
) -> Option<SecondaryActionOutcome> {
    let pending = take_player_pending_secondary_use(state)?;
    let current_position = state
        .logistics()
        .player()
        .map(|player| player.body().foot_position());
    if current_position != Some(pending.position())
        || state.logistics().active_selection() != pending.selection()
    {
        return Some(SecondaryActionOutcome::BufferedContextChanged);
    }
    let direction = pending.direction().unwrap_or_else(|| {
        panic!("runtime invariant broken: trusted buffered contextual use has invalid direction")
    });
    Some(attempt_secondary_action_now(registries, state, direction))
}
