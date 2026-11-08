//! Shared pointed-placement targeting, geometry, and structural dismantling.

use std::collections::BTreeSet;

use crate::content::STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER;
use crate::core::state::AppState;
use crate::logistics::player_body_occupies_voxel;
use crate::registry::Registries;
use crate::spatial::{CubeFace, ViewDirection, VoxelBounds, VoxelCoord};
use crate::structural::{StructuralElementId, validate_start_player_structural_dismantling};
use crate::world::{BlockStateId, PlayerBlockTargetError, raycast_player_block};

use super::{
    DirectStructuralTargetError, PrimaryActionOutcome, SecondaryActionOutcome,
    resolve_direct_structural_face_target,
};

mod storage;
mod timber;
pub(super) use storage::{attempt_selected_storage_placement, attempt_storage_placement};
pub(super) use timber::{
    attempt_selected_timber_cover_placement, attempt_selected_timber_placement,
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
    if state.world().tree_trunk_at(position).is_some()
        || crate::kobolds::temple_caste_occupies_voxel(registries, state, position)
        || crate::kobolds::opening_kobold_occupies_voxel(state, position)
        || state.logistics().has_stationary_occupant_at(position)
        || state
            .structures()
            .elements_covering(position)
            .any(|member| !member.embodied_material().is_empty())
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
        || state.surface().rooted_biomass_occupies_voxel(position)
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
    // An unbuilt plan has no physical surface to place against. Logs can
    // still resolve a real terrain face beyond it; boards require real framing.
    if state
        .structures()
        .get_element(target.element)
        .is_none_or(|member| member.embodied_material().is_empty())
    {
        return Ok(None);
    }
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

#[cfg(test)]
#[path = "placement_tests.rs"]
mod tests;
