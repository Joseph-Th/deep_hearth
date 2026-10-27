//! Canonical pointed placement of structural members and storage, plus targeted dismantling.

use std::collections::BTreeSet;

use crate::content::{
    BASIC_TIMBER_COVER_PANEL_AREA, BASIC_TIMBER_COVER_PANEL_THICKNESS,
    BASIC_TIMBER_POST_CROSS_SECTION, BASIC_TIMBER_POST_LENGTH, FORM_BOARD, FORM_LOG, MATERIAL_WOOD,
    STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER, STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
};
use crate::core::state::AppState;
use crate::inventory::{StorageDefinitionId, validate_place_player_storage_at};
use crate::logistics::player_body_occupies_voxel;
use crate::material::CommodityKey;
use crate::registry::Registries;
use crate::spatial::{CubeFace, ViewDirection, VoxelBounds, VoxelCoord};
use crate::structural::{
    GroundedStructuralElementPlan, MultiSupportedStructuralElementPlan, StructuralElementGeometry,
    StructuralElementId, validate_start_player_grounded_structural_construction_from_stack,
    validate_start_player_multi_supported_structural_construction_from_stack,
    validate_start_player_structural_dismantling,
};
use crate::world::{
    BlockCollisionShape, BlockStateId, PlayerBlockTargetError, raycast_player_block,
};

use super::secondary::resolve_unique_carried_stack;
use super::{
    DirectStructuralTargetError, PrimaryActionOutcome, SecondaryActionOutcome,
    resolve_direct_structural_face_target,
};

fn validate_direct_placement_position(
    registries: &Registries,
    state: &AppState,
    position: VoxelCoord,
) -> Result<(), DirectPlacementTargetError> {
    match state.world().block_at(position) {
        None => return Err(DirectPlacementTargetError::WorldUnavailable { position }),
        Some(BlockStateId::AIR) => {}
        Some(block) => {
            return Err(DirectPlacementTargetError::Occupied {
                position,
                state: block,
            });
        }
    }
    if player_body_occupies_voxel(registries, state, position) {
        return Err(DirectPlacementTargetError::IntersectsPlayer { position });
    }
    if crate::kobolds::temple_caste_occupies_voxel(registries, state, position)
        || crate::kobolds::opening_kobold_occupies_voxel(state, position)
        || state.logistics().has_stationary_occupant_at(position)
        || state
            .structures()
            .elements_covering(position)
            .next()
            .is_some()
    {
        return Err(DirectPlacementTargetError::LocationOccupied { position });
    }
    Ok(())
}

fn validate_structural_placement_position(
    state: &AppState,
    position: VoxelCoord,
) -> Result<(), DirectPlacementTargetError> {
    if state
        .surface()
        .available_resources_at(Some(position))
        .next()
        .is_some()
    {
        return Err(DirectPlacementTargetError::LocationOccupied { position });
    }
    Ok(())
}

fn structural_placement_error(error: DirectPlacementTargetError) -> SecondaryActionOutcome {
    match error {
        DirectPlacementTargetError::Target(error) => {
            SecondaryActionOutcome::StructuralTargetRejected(error)
        }
        DirectPlacementTargetError::NoTarget => SecondaryActionOutcome::NoStructuralPlacementTarget,
        DirectPlacementTargetError::CoordinateOverflow => {
            SecondaryActionOutcome::StructuralPlacementCoordinateOverflow
        }
        DirectPlacementTargetError::WorldUnavailable { position } => {
            SecondaryActionOutcome::StructuralPlacementWorldUnavailable { position }
        }
        DirectPlacementTargetError::Occupied { position, state } => {
            SecondaryActionOutcome::StructuralPlacementOccupied { position, state }
        }
        DirectPlacementTargetError::IntersectsPlayer { position } => {
            SecondaryActionOutcome::StructuralPlacementIntersectsPlayer { position }
        }
        DirectPlacementTargetError::LocationOccupied { position } => {
            SecondaryActionOutcome::StructuralPlacementLocationOccupied { position }
        }
    }
}

/// Resolves shared pointed structural support and adjacent free geometry for timber members.
/// Absence is retained so logs can fall back to terrain, while boards require a member target.
fn resolve_supported_timber_target(
    registries: &Registries,
    state: &AppState,
    direction: ViewDirection,
) -> Result<Option<(VoxelBounds, Vec<StructuralElementId>)>, SecondaryActionOutcome> {
    let target = match resolve_direct_structural_face_target(registries, state, direction) {
        Ok(Some(target)) => target,
        Ok(None)
        | Err(DirectStructuralTargetError::TargetUnavailable)
        | Err(DirectStructuralTargetError::NoTarget) => return Ok(None),
        Err(DirectStructuralTargetError::Ambiguous { count }) => {
            return Err(SecondaryActionOutcome::AmbiguousStructuralPlacementTarget { count });
        }
    };
    let position = target
        .face
        .adjacent_voxel(target.voxel)
        .ok_or(SecondaryActionOutcome::StructuralPlacementCoordinateOverflow)?;
    validate_direct_placement_position(registries, state, position)
        .map_err(structural_placement_error)?;
    let bounds = single_voxel_bounds(position)
        .ok_or(SecondaryActionOutcome::StructuralPlacementCoordinateOverflow)?;
    Ok(Some((
        bounds,
        local_active_face_supports(state, position, bounds),
    )))
}

/// Resolve supported placement from the structural spatial index, not a global member scan.
/// The caller has already proved the proposed single voxel is free of structural occupants.
fn local_active_face_supports(
    state: &AppState,
    position: VoxelCoord,
    bounds: VoxelBounds,
) -> Vec<StructuralElementId> {
    // Only members covering a face-neighbor cell can touch this empty voxel.
    // Stable deduplication matters when an oversized member covers several neighbors.
    let mut supports = BTreeSet::new();
    for face in [
        CubeFace::Top,
        CubeFace::Bottom,
        CubeFace::North,
        CubeFace::South,
        CubeFace::East,
        CubeFace::West,
    ] {
        let Some(neighbor) = face.adjacent_voxel(position) else {
            continue;
        };
        for record in state.structures().elements_covering(neighbor) {
            if record.lifecycle() == crate::structural::StructuralLifecycle::Active
                && bounds.has_face_contact(record.bounds())
            {
                supports.insert(record.id());
            }
        }
    }
    supports.into_iter().collect()
}

fn attempt_supported_timber_placement(
    registries: &Registries,
    state: &mut AppState,
    stack: &crate::inventory::MaterialStackProjection,
    direction: ViewDirection,
) -> Option<SecondaryActionOutcome> {
    let (bounds, supports) = match resolve_supported_timber_target(registries, state, direction) {
        Ok(Some(target)) => target,
        Ok(None) => return None,
        Err(error) => return Some(error),
    };
    let geometry = StructuralElementGeometry::new(
        bounds,
        BASIC_TIMBER_POST_LENGTH,
        BASIC_TIMBER_POST_CROSS_SECTION,
    )
    .unwrap_or_else(|error| panic!("built-in supported timber geometry is invalid: {error}"));
    let plan = MultiSupportedStructuralElementPlan::new(
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        geometry,
        supports,
    );
    Some(
        match validate_start_player_multi_supported_structural_construction_from_stack(
            registries,
            state,
            STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
            plan,
            stack,
        ) {
            Ok(validated) => {
                let work = validated.commit(state).unwrap_or_else(|error| {
                    panic!(
                        "runtime invariant broken: synchronous supported timber placement became stale: {error}"
                    )
                });
                SecondaryActionOutcome::StructuralConstructionStarted(work)
            }
            Err(error) => SecondaryActionOutcome::StructuralPlacementRejected(error),
        },
    )
}

pub(super) fn attempt_structural_dismantling_now(
    registries: &Registries,
    state: &mut AppState,
    element: StructuralElementId,
) -> PrimaryActionOutcome {
    match validate_start_player_structural_dismantling(
        registries,
        state,
        element,
        STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
    ) {
        Ok(validated) => {
            let outcome = validated.commit(state).unwrap_or_else(|error| {
                panic!(
                    "runtime invariant broken: synchronous structural dismantling became stale: {error}"
                )
            });
            PrimaryActionOutcome::StructuralDismantlingStarted(outcome)
        }
        Err(error) => PrimaryActionOutcome::StructuralDismantlingRejected(error),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum DirectPlacementTargetError {
    Target(PlayerBlockTargetError),
    NoTarget,
    CoordinateOverflow,
    WorldUnavailable {
        position: VoxelCoord,
    },
    Occupied {
        position: VoxelCoord,
        state: BlockStateId,
    },
    IntersectsPlayer {
        position: VoxelCoord,
    },
    LocationOccupied {
        position: VoxelCoord,
    },
}

fn resolve_direct_placement_target(
    registries: &Registries,
    state: &AppState,
    direction: ViewDirection,
) -> Result<(CubeFace, VoxelCoord), DirectPlacementTargetError> {
    let target = match raycast_player_block(registries, state, direction) {
        Ok(Some(target)) => target,
        Ok(None) => return Err(DirectPlacementTargetError::NoTarget),
        Err(error) => return Err(DirectPlacementTargetError::Target(error)),
    };
    let position = target
        .face()
        .adjacent_voxel(target.voxel())
        .ok_or(DirectPlacementTargetError::CoordinateOverflow)?;
    validate_direct_placement_position(registries, state, position)?;
    Ok((target.face(), position))
}

fn single_voxel_bounds(position: VoxelCoord) -> Option<VoxelBounds> {
    let max = VoxelCoord::new(
        position.x().checked_add(1)?,
        position.y().checked_add(1)?,
        position.z().checked_add(1)?,
    );
    VoxelBounds::new(position, max).ok()
}

pub(super) fn attempt_selected_timber_cover_placement(
    registries: &Registries,
    state: &mut AppState,
    commodity: CommodityKey,
    direction: ViewDirection,
) -> Option<SecondaryActionOutcome> {
    if commodity != CommodityKey::new(MATERIAL_WOOD, FORM_BOARD) {
        return None;
    }
    if !state.control_tick().is_simulation_boundary() {
        return Some(SecondaryActionOutcome::DeferredUntilSimulationBoundary);
    }
    let stack = match resolve_unique_carried_stack(registries, state, commodity) {
        Ok(stack) => stack,
        Err(outcome) => return Some(outcome),
    };
    let stack = &stack;
    let (bounds, supports) = match resolve_supported_timber_target(registries, state, direction) {
        Ok(Some(target)) => target,
        Ok(None) => return Some(SecondaryActionOutcome::NoStructuralPlacementTarget),
        Err(error) => return Some(error),
    };
    if let Err(error) = validate_structural_placement_position(state, bounds.min()) {
        return Some(structural_placement_error(error));
    }
    let geometry = StructuralElementGeometry::new(
        bounds,
        BASIC_TIMBER_COVER_PANEL_THICKNESS,
        BASIC_TIMBER_COVER_PANEL_AREA,
    )
    .unwrap_or_else(|error| panic!("built-in timber cover-panel geometry is invalid: {error}"));
    let plan = MultiSupportedStructuralElementPlan::new(
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        geometry,
        supports,
    );
    Some(
        match validate_start_player_multi_supported_structural_construction_from_stack(
            registries,
            state,
            STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
            plan,
            stack,
        ) {
            Ok(validated) => {
                let work = validated.commit(state).unwrap_or_else(|error| {
                    panic!(
                        "runtime invariant broken: synchronous timber cover placement became stale: {error}"
                    )
                });
                SecondaryActionOutcome::StructuralConstructionStarted(work)
            }
            Err(error) => SecondaryActionOutcome::StructuralPlacementRejected(error),
        },
    )
}

pub(super) fn attempt_selected_timber_placement(
    registries: &Registries,
    state: &mut AppState,
    commodity: CommodityKey,
    direction: ViewDirection,
) -> Option<SecondaryActionOutcome> {
    if commodity != CommodityKey::new(MATERIAL_WOOD, FORM_LOG) {
        return None;
    }
    if !state.control_tick().is_simulation_boundary() {
        return Some(SecondaryActionOutcome::DeferredUntilSimulationBoundary);
    }
    let stack = match resolve_unique_carried_stack(registries, state, commodity) {
        Ok(stack) => stack,
        Err(outcome) => return Some(outcome),
    };
    let stack = &stack;
    if let Some(outcome) = attempt_supported_timber_placement(registries, state, stack, direction) {
        return Some(outcome);
    }
    let (face, position) = match resolve_direct_placement_target(registries, state, direction) {
        Ok(target) => target,
        Err(error) => return Some(structural_placement_error(error)),
    };
    if face != CubeFace::Top {
        return Some(SecondaryActionOutcome::StructuralPlacementRequiresTopFace { face });
    }
    let Some(bounds) = single_voxel_bounds(position) else {
        return Some(SecondaryActionOutcome::StructuralPlacementCoordinateOverflow);
    };
    let geometry = StructuralElementGeometry::new(
        bounds,
        BASIC_TIMBER_POST_LENGTH,
        BASIC_TIMBER_POST_CROSS_SECTION,
    )
    .unwrap_or_else(|error| panic!("built-in timber-post geometry is invalid: {error}"));
    let plan = GroundedStructuralElementPlan::new(
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        geometry,
    );
    Some(
        match validate_start_player_grounded_structural_construction_from_stack(
            registries,
            state,
            STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
            plan,
            stack,
        ) {
            Ok(validated) => {
                let work = validated.commit(state).unwrap_or_else(|error| {
                    panic!(
                        "runtime invariant broken: synchronous timber placement became stale: {error}"
                    )
                });
                SecondaryActionOutcome::StructuralConstructionStarted(work)
            }
            Err(error) => SecondaryActionOutcome::StructuralPlacementRejected(error),
        },
    )
}

pub(super) fn attempt_selected_storage_placement(
    registries: &Registries,
    state: &mut AppState,
    commodity: crate::material::CommodityKey,
    direction: crate::spatial::ViewDirection,
) -> Option<SecondaryActionOutcome> {
    let definitions = registries
        .storage()
        .definitions()
        .filter(|definition| {
            let inputs = definition.assembly_profile().inputs();
            inputs.len() == 1 && inputs[0].commodity() == commodity
        })
        .map(|definition| definition.id())
        .collect::<Vec<StorageDefinitionId>>();
    let definition = match definitions.as_slice() {
        [] => return None,
        [definition] => *definition,
        definitions => {
            return Some(SecondaryActionOutcome::AmbiguousStorageDefinitions {
                count: definitions.len(),
            });
        }
    };
    Some(attempt_storage_placement(
        registries, state, definition, direction,
    ))
}

/// Authoritative pointed placement shared by one-piece selected-material shortcuts and explicit
/// multi-input storage-plan actions. Both use the same terrain, occupation and construction rules.
pub(super) fn attempt_storage_placement(
    registries: &Registries,
    state: &mut AppState,
    definition: StorageDefinitionId,
    direction: crate::spatial::ViewDirection,
) -> SecondaryActionOutcome {
    let (_, position) = match resolve_direct_placement_target(registries, state, direction) {
        Ok(target) => target,
        Err(DirectPlacementTargetError::Target(error)) => {
            return SecondaryActionOutcome::StorageTargetRejected(error);
        }
        Err(DirectPlacementTargetError::NoTarget) => {
            return SecondaryActionOutcome::NoStoragePlacementTarget;
        }
        Err(DirectPlacementTargetError::CoordinateOverflow) => {
            return SecondaryActionOutcome::StoragePlacementCoordinateOverflow;
        }
        Err(DirectPlacementTargetError::WorldUnavailable { position }) => {
            return SecondaryActionOutcome::StoragePlacementWorldUnavailable { position };
        }
        Err(DirectPlacementTargetError::Occupied { position, state }) => {
            return SecondaryActionOutcome::StoragePlacementOccupied { position, state };
        }
        Err(DirectPlacementTargetError::IntersectsPlayer { position }) => {
            return SecondaryActionOutcome::StoragePlacementIntersectsPlayer { position };
        }
        Err(DirectPlacementTargetError::LocationOccupied { position }) => {
            return SecondaryActionOutcome::StoragePlacementLocationOccupied { position };
        }
    };
    // Pointed placement must not suspend a freestanding container in empty air. The lower-level
    // stockpile allocator deliberately owns custody rather than terrain physics; this direct
    // interaction owns the generated-world contact requirement.
    let supported = position
        .y()
        .checked_sub(1)
        .and_then(|y| {
            state
                .world()
                .block_at(VoxelCoord::new(position.x(), y, position.z()))
        })
        .and_then(|block| registries.world().block_for_state(block))
        .is_some_and(|block| block.collision() == BlockCollisionShape::FullCube);
    if !supported {
        return SecondaryActionOutcome::StoragePlacementUnsupported { position };
    }
    match validate_place_player_storage_at(registries, state, definition, position) {
        Ok(validated) => {
            let stockpile = validated.commit(state).unwrap_or_else(|error| {
                    panic!(
                        "runtime invariant broken: synchronous selected-storage placement became stale: {error}"
                    )
                });
            SecondaryActionOutcome::StoragePlaced {
                definition,
                stockpile,
            }
        }
        Err(error) => SecondaryActionOutcome::StoragePlacementRejected { definition, error },
    }
}

#[cfg(test)]
#[path = "placement_tests.rs"]
mod tests;
