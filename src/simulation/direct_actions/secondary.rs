//! Selected-material contextual secondary use and owner-specific action dispatch.

use crate::content::{FORM_BOARD, FORM_LOG, MATERIAL_WOOD};
use crate::core::state::AppState;
use crate::inventory::{
    MaterialStackChoice, MaterialStackProjection, resolve_material_stack_choice,
};
use crate::logistics::PlayerActiveSelection;
use crate::material::CommodityKey;
use crate::player::{PlayerActionValidationError, validate_player_action};
use crate::registry::Registries;
use crate::spatial::ViewDirection;
use crate::survival::{EatStackToTargetError, validate_eat_stack_to_full_control_rate};
use crate::world::raycast_player_block;

use super::SecondaryActionOutcome;
use super::placement::{
    attempt_selected_storage_placement, attempt_selected_timber_cover_placement,
    attempt_selected_timber_placement,
};

mod combustion;
mod gathering;
mod material_choice;

use combustion::{attempt_selected_combustion, has_selected_combustion_target};
pub(super) use gathering::{
    attempt_empty_hand_secondary_action, attempt_explicit_surface_gathering,
    attempt_selected_drinking, attempt_selected_surface_gathering,
};
pub(super) use material_choice::resolve_unique_carried_stack;

pub(super) fn attempt_selected_secondary_action(
    registries: &Registries,
    state: &mut AppState,
    selection: PlayerActiveSelection,
    direction: ViewDirection,
) -> SecondaryActionOutcome {
    let commodity = match selection {
        PlayerActiveSelection::Equipment(_) => {
            return SecondaryActionOutcome::ActiveEquipmentNotContextual;
        }
        PlayerActiveSelection::Material(commodity) => commodity,
    };
    if let Some(outcome) =
        attempt_selected_storage_placement(registries, state, commodity, direction)
    {
        return outcome;
    }
    let points_at_block = matches!(
        raycast_player_block(registries, state, direction),
        Ok(Some(_))
    );
    let timber = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    if commodity == timber
        && points_at_block
        && let Some(outcome) =
            attempt_selected_timber_placement(registries, state, commodity, direction)
    {
        // An impossible pointed post (including a side-face aim or an obstructed
        // site) cannot shadow a viable local fuel use. Valid construction and
        // its deferred admission keep priority over co-located combustion.
        if matches!(
            outcome,
            SecondaryActionOutcome::StructuralPlacementIntersectsPlayer { .. }
                | SecondaryActionOutcome::StructuralPlacementRequiresTopFace { .. }
                | SecondaryActionOutcome::StructuralPlacementLocationOccupied { .. }
                | SecondaryActionOutcome::StructuralPlacementOccupied { .. }
                | SecondaryActionOutcome::StructuralPlacementWorldUnavailable { .. }
                | SecondaryActionOutcome::StructuralPlacementCoordinateOverflow
        ) {
            // Fuel transactions remain on the slow boundary even when the
            // impossible construction attempt rejects on the input frame.
            if !state.control_tick().is_simulation_boundary()
                && has_selected_combustion_target(registries, state, commodity)
            {
                return SecondaryActionOutcome::DeferredUntilSimulationBoundary;
            }
            if let Some(combustion) = attempt_selected_combustion(registries, state, commodity) {
                return combustion;
            }
        }
        return outcome;
    }
    if commodity == CommodityKey::new(MATERIAL_WOOD, FORM_BOARD)
        && let Some(outcome) =
            attempt_selected_timber_cover_placement(registries, state, commodity, direction)
    {
        return outcome;
    }
    if let Some(outcome) = attempt_selected_combustion(registries, state, commodity) {
        return outcome;
    }
    if let Some(outcome) =
        attempt_selected_timber_placement(registries, state, commodity, direction)
    {
        return outcome;
    }
    if registries.survival().get_food(commodity).is_none() {
        return SecondaryActionOutcome::MaterialNotContextual;
    }
    let stack = match resolve_unique_carried_stack(registries, state, commodity) {
        Ok(stack) => stack,
        Err(outcome) => return outcome,
    };
    eat_stack(registries, state, &stack)
}

/// Explicit stack choices use the same direct eating transaction as selected
/// food, but preserve the chosen physical cohort when the commodity is ambiguous.
pub(super) fn attempt_selected_eating(
    registries: &Registries,
    state: &mut AppState,
    choice: MaterialStackChoice,
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
    let stack = match resolve_material_stack_choice(registries, state, choice) {
        Ok(stack) => stack,
        Err(error) => {
            return SecondaryActionOutcome::EatingRejected(EatStackToTargetError::Stack(error));
        }
    };
    if registries.survival().get_food(stack.commodity()).is_none() {
        return SecondaryActionOutcome::EatingRejected(EatStackToTargetError::NotEdible {
            commodity: stack.commodity(),
        });
    }
    eat_stack(registries, state, &stack)
}

fn eat_stack(
    registries: &Registries,
    state: &mut AppState,
    stack: &MaterialStackProjection,
) -> SecondaryActionOutcome {
    // Hotbar bindings survive depletion, but an actual meal starts on
    // the input frame with canonical custody and survival admission.
    match validate_eat_stack_to_full_control_rate(registries, state, stack) {
        Ok(Some(validated)) => {
            let outcome = validated.commit(state).unwrap_or_else(|error| {
                panic!(
                    "runtime invariant broken: synchronous selected-food use became stale: {error}"
                )
            });
            SecondaryActionOutcome::EatingStarted(outcome)
        }
        Ok(None) => SecondaryActionOutcome::AlreadySatisfied,
        Err(error) => SecondaryActionOutcome::EatingRejected(error),
    }
}

#[cfg(test)]
#[path = "secondary_tests.rs"]
mod tests;
