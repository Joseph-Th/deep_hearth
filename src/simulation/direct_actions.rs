//! Ordered direct player interactions evaluated after authoritative body motion.

mod placement;
mod secondary;

use placement::{attempt_storage_placement, attempt_structural_dismantling_now};
use secondary::{attempt_empty_hand_secondary_action, attempt_selected_secondary_action};

use crate::combat::{validate_primitive_shove, validate_primitive_strike};
use crate::core::state::AppState;
use crate::logistics::{
    PlayerActiveSelection, take_player_pending_primary_break, take_player_pending_secondary_use,
    validate_buffer_player_primary_break, validate_buffer_player_secondary_use,
    validate_drop_equipment, validate_select_quick_access_slot,
};
use crate::player::{PlayerActionValidationError, validate_player_action};
use crate::registry::Registries;
use crate::spatial::{CubeFace, ViewDirection, VoxelCoord};
use crate::structural::StructuralElementId;
use crate::surface::{PlayerMaterialDropOutcome, validate_drop_player_material};
use crate::world::{raycast_player_air_voxel, validate_start_player_block_excavation_active};

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
    let primary = if pending_primary.is_some() {
        pending_primary
    } else if state.player_work().active().is_none() {
        attempt_primary_action(registries, state, input.primary())
    } else {
        None
    };
    let secondary = if state.player_work().active().is_some() {
        pending_secondary
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
    if state.logistics().pending_secondary_use().is_some() {
        return Some(SecondaryActionOutcome::QueuedUntilSimulationBoundary);
    }
    let direction = match intent {
        SecondaryControlIntent::UseActive { direction } => direction,
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
    Some(
        match validate_buffer_player_secondary_use(state, direction) {
            Ok(validated) => {
                validated.commit(state).unwrap_or_else(|error| {
                    panic!(
                        "runtime invariant broken: synchronous contextual-use buffering became stale: {error}"
                    )
                });
                SecondaryActionOutcome::QueuedUntilSimulationBoundary
            }
            Err(error) => SecondaryActionOutcome::BufferRejected(error),
        },
    )
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
    let current_position = state.logistics().player().map(|player| player.position());
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

pub(super) fn attempt_pending_primary_break(
    registries: &Registries,
    state: &mut AppState,
) -> Option<PrimaryActionOutcome> {
    let pending = take_player_pending_primary_break(state)?;
    let current_position = state.logistics().player().map(|player| player.position());
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

enum DirectStructuralTargetError {
    TargetUnavailable,
    NoTarget,
    Ambiguous { count: usize },
}

fn resolve_direct_structural_target(
    registries: &Registries,
    state: &AppState,
    direction: ViewDirection,
) -> Result<StructuralElementId, DirectStructuralTargetError> {
    resolve_direct_structural_face_target(registries, state, direction)?
        .map(|target| target.element)
        .ok_or(DirectStructuralTargetError::NoTarget)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DirectStructuralFaceTarget {
    element: StructuralElementId,
    voxel: VoxelCoord,
    face: CubeFace,
}

fn resolve_direct_structural_face_target(
    registries: &Registries,
    state: &AppState,
    direction: ViewDirection,
) -> Result<Option<DirectStructuralFaceTarget>, DirectStructuralTargetError> {
    let target = raycast_player_air_voxel(registries, state, direction, |voxel, face| {
        // Physical matter takes precedence over inert plans occupying the same
        // cell. Both may coexist, but only materialized members can physically
        // occlude a plan. Preserve ambiguity among multiple real members.
        let mut embodied = (None, 0_usize);
        let mut planned = (None, 0_usize);
        for record in state
            .structures()
            .elements()
            .filter(|record| record.bounds().has_voxel(voxel))
        {
            let group = if record.embodied_material().is_empty() {
                &mut planned
            } else {
                &mut embodied
            };
            group.0.get_or_insert(record.id());
            group.1 += 1;
        }
        let (first, count) = if embodied.1 != 0 { embodied } else { planned };
        let first = first?;
        Some(if count == 1 {
            Ok(DirectStructuralFaceTarget {
                element: first,
                voxel,
                face,
            })
        } else {
            Err(count)
        })
    })
    .map_err(|_| DirectStructuralTargetError::TargetUnavailable)?;
    target
        .transpose()
        .map_err(|count| DirectStructuralTargetError::Ambiguous { count })
}

fn attempt_primary_break(
    registries: &Registries,
    state: &mut AppState,
    direction: ViewDirection,
) -> PrimaryActionOutcome {
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
