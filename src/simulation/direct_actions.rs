//! Ordered direct player interactions evaluated after authoritative body motion.

mod placement;
mod secondary;
mod structural_target;

use placement::{attempt_storage_placement, attempt_structural_dismantling_now};
use secondary::{
    attempt_empty_hand_secondary_action, attempt_explicit_surface_gathering,
    attempt_selected_secondary_action,
};
use structural_target::{
    DirectStructuralTargetError, resolve_direct_structural_face_target,
    resolve_direct_structural_target,
};

use crate::combat::{
    validate_cancel_primitive_preparation, validate_prepare_primitive_strike,
    validate_primitive_shove, validate_primitive_strike, validate_release_prepared_strike,
};
use crate::core::state::AppState;
use crate::logistics::{
    PlayerActiveSelection, take_player_pending_primary_break, take_player_pending_secondary_use,
    validate_buffer_player_primary_break, validate_buffer_player_secondary_surface_gathering,
    validate_buffer_player_secondary_use, validate_drop_equipment,
    validate_select_quick_access_slot,
};
use crate::player::{PlayerActionValidationError, validate_player_action};
use crate::registry::Registries;
use crate::spatial::ViewDirection;
use crate::surface::{PlayerMaterialDropOutcome, validate_drop_player_material};
use crate::world::validate_start_player_block_excavation_active;

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
    // A different explicit one-shot action supersedes an older buffered action in
    // the same control slot. Otherwise the obsolete action would execute on the next
    // slow boundary even after the newer input was handled. Both buffered owners
    // reserve the revision needed for this early cancellation.
    if combat_intent {
        let _ = take_player_pending_primary_break(state);
    }
    if input.drop_active_selection()
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

fn attempt_primary_action(
    registries: &Registries,
    state: &mut AppState,
    intent: Option<PrimaryControlIntent>,
) -> Option<PrimaryActionOutcome> {
    let intent = intent?;
    match validate_player_action(state) {
        Ok(_) => {}
        Err(PlayerActionValidationError::PlayerNotAdmitted) => {
            return Some(PrimaryActionOutcome::PlayerNotAdmitted);
        }
        Err(PlayerActionValidationError::PlayerDead) => {
            return Some(PrimaryActionOutcome::PlayerDead);
        }
    }
    match intent {
        PrimaryControlIntent::Shove { direction } => Some(
            match validate_primitive_shove(registries, state, direction) {
                Ok(validated) => {
                    PrimaryActionOutcome::Shove(validated.commit(state).unwrap_or_else(|error| {
                        panic!("synchronous primitive shove became stale: {error:?}")
                    }))
                }
                Err(error) => PrimaryActionOutcome::ShoveRejected(error),
            },
        ),
        PrimaryControlIntent::Strike { direction } => Some(
            match validate_primitive_strike(registries, state, direction) {
                Ok(validated) => {
                    PrimaryActionOutcome::Strike(validated.commit(state).unwrap_or_else(|error| {
                        panic!("synchronous primitive weapon contact became stale: {error:?}")
                    }))
                }
                Err(error) => PrimaryActionOutcome::StrikeRejected(error),
            },
        ),
        PrimaryControlIntent::PrepareStrike { direction } => Some(
            match validate_prepare_primitive_strike(registries, state, direction) {
                Ok(validated) => {
                    PrimaryActionOutcome::Preparation(validated.commit(state).unwrap_or_else(
                        |error| panic!("synchronous strike preparation became stale: {error:?}"),
                    ))
                }
                Err(error) => PrimaryActionOutcome::PreparationRejected(error),
            },
        ),
        PrimaryControlIntent::ReleaseStrike { direction } => Some(
            match validate_release_prepared_strike(registries, state, direction) {
                Ok(validated) => {
                    PrimaryActionOutcome::PreparedStrike(validated.commit(state).unwrap_or_else(
                        |error| panic!("synchronous prepared strike became stale: {error:?}"),
                    ))
                }
                Err(error) => PrimaryActionOutcome::PreparedStrikeRejected(error),
            },
        ),
        PrimaryControlIntent::CancelStrike => {
            Some(match validate_cancel_primitive_preparation(state) {
                Ok(validated) => PrimaryActionOutcome::Preparation(
                    validated.commit(state).unwrap_or_else(|error| {
                        panic!("synchronous preparation cancellation became stale: {error:?}")
                    }),
                ),
                Err(error) => PrimaryActionOutcome::PreparationRejected(error),
            })
        }
        PrimaryControlIntent::BreakTarget { direction } => {
            Some(attempt_primary_break(registries, state, direction))
        }
    }
}

fn attempt_secondary_action(
    registries: &Registries,
    state: &mut AppState,
    intent: Option<SecondaryControlIntent>,
) -> Option<SecondaryActionOutcome> {
    let intent = intent?;
    let requested_surface = match intent {
        SecondaryControlIntent::GatherSurface { resource, mass, .. } => Some((resource, mass)),
        SecondaryControlIntent::UseActive { .. } | SecondaryControlIntent::PlaceStorage { .. } => {
            None
        }
    };
    if let Some(pending) = state.logistics().pending_secondary_use() {
        if matches!(
            intent,
            SecondaryControlIntent::UseActive { direction }
                | SecondaryControlIntent::GatherSurface { direction, .. }
                if pending.direction() == Some(direction)
                    && pending.surface_request() == requested_surface
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
        SecondaryControlIntent::UseActive { direction }
        | SecondaryControlIntent::GatherSurface { direction, .. } => direction,
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
    let outcome = match requested_surface {
        Some((resource, mass)) => {
            attempt_explicit_surface_gathering(registries, state, resource, mass)
        }
        None => attempt_secondary_action_now(registries, state, direction),
    };
    if outcome != SecondaryActionOutcome::DeferredUntilSimulationBoundary {
        return Some(outcome);
    }
    let buffered = match requested_surface {
        Some((resource, mass)) => {
            validate_buffer_player_secondary_surface_gathering(state, direction, resource, mass)
        }
        None => validate_buffer_player_secondary_use(state, direction),
    };
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

pub(super) fn attempt_pending_secondary_action(
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
    Some(match pending.surface_request() {
        Some((resource, mass)) => {
            attempt_explicit_surface_gathering(registries, state, resource, mass)
        }
        None => attempt_secondary_action_now(registries, state, direction),
    })
}

pub(super) fn attempt_pending_primary_break(
    registries: &Registries,
    state: &mut AppState,
) -> Option<PrimaryActionOutcome> {
    let pending = take_player_pending_primary_break(state)?;
    let current_position = state
        .logistics()
        .player()
        .map(|player| player.body().foot_position());
    if current_position != Some(pending.position()) {
        return Some(PrimaryActionOutcome::BufferedContextChanged);
    }
    let direction = pending.direction().unwrap_or_else(|| {
        panic!("runtime invariant broken: trusted buffered structural break has invalid direction")
    });
    match resolve_direct_structural_target(registries, state, direction) {
        Ok(element) if element == pending.element() => Some(attempt_structural_dismantling_now(
            registries, state, element,
        )),
        Ok(_)
        | Err(DirectStructuralTargetError::TargetUnavailable)
        | Err(DirectStructuralTargetError::NoTarget)
        | Err(DirectStructuralTargetError::Ambiguous { .. }) => {
            Some(PrimaryActionOutcome::BufferedContextChanged)
        }
    }
}

fn attempt_primary_break(
    registries: &Registries,
    state: &mut AppState,
    direction: ViewDirection,
) -> PrimaryActionOutcome {
    if let Some(pending) = state.logistics().pending_primary_break() {
        if pending.direction() == Some(direction)
            && state
                .logistics()
                .player()
                .map(|player| player.body().foot_position())
                == Some(pending.position())
        {
            // Held breaking is one scheduled action, not repeated failed admissions.
            // Target/world changes are still revalidated on its due boundary.
            return PrimaryActionOutcome::QueuedUntilSimulationBoundary;
        }
        // A new direction or actor location replaces the stale one-shot request.
        let _ = take_player_pending_primary_break(state);
    }
    let element = match resolve_direct_structural_target(registries, state, direction) {
        Ok(element) => Some(element),
        Err(DirectStructuralTargetError::Ambiguous { count }) => {
            return PrimaryActionOutcome::AmbiguousStructuralTarget { count };
        }
        Err(
            DirectStructuralTargetError::TargetUnavailable | DirectStructuralTargetError::NoTarget,
        ) => None,
    };

    if let Some(element) = element {
        if !state.control_tick().is_simulation_boundary() {
            return match validate_buffer_player_primary_break(state, element, direction) {
                Ok(validated) => {
                    validated.commit(state).unwrap_or_else(|error| {
                        panic!(
                            "runtime invariant broken: synchronous structural-break buffering became stale: {error}"
                        )
                    });
                    PrimaryActionOutcome::QueuedUntilSimulationBoundary
                }
                Err(error) => PrimaryActionOutcome::BufferRejected(error),
            };
        }
        return attempt_structural_dismantling_now(registries, state, element);
    }

    match validate_start_player_block_excavation_active(registries, state, direction) {
        Ok(validated) => {
            let started = validated.commit(state).unwrap_or_else(|error| {
                panic!(
                    "runtime invariant broken: synchronous primary block excavation became stale: {error}"
                )
            });
            PrimaryActionOutcome::BlockExcavationStarted(started)
        }
        Err(error) => PrimaryActionOutcome::BlockExcavationRejected(error),
    }
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
