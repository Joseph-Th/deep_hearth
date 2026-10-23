//! Bounded local percussion through actual open voxels, independently of sight.
//!
//! A sound may follow a bent air corridor but not cut across solid rock, built panels, or an
//! unknown/unloaded chunk. With no generated terrain, the opening wilderness
//! uses only a short Manhattan-radius approximation. This is not a world-wide
//! noise map or a signal network; rig transmission has separate physical rules.

use std::collections::{BTreeSet, VecDeque};

use crate::core::state::AppState;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::world::BlockCollisionShape;

use super::junkworks::{KoboldJunkRig, KoboldJunkRigKind};

fn acoustic_reach(kind: KoboldJunkRigKind) -> u8 {
    match kind {
        // The wooden brace dampens the crack; a bare stone clapper carries farther.
        KoboldJunkRigKind::TimberRatchet => 3,
        KoboldJunkRigKind::StoneClatter => 5,
    }
}

fn neighbors(voxel: VoxelCoord) -> impl Iterator<Item = VoxelCoord> {
    [
        voxel
            .x()
            .checked_add(1)
            .map(|x| VoxelCoord::new(x, voxel.y(), voxel.z())),
        voxel
            .x()
            .checked_sub(1)
            .map(|x| VoxelCoord::new(x, voxel.y(), voxel.z())),
        voxel
            .y()
            .checked_add(1)
            .map(|y| VoxelCoord::new(voxel.x(), y, voxel.z())),
        voxel
            .y()
            .checked_sub(1)
            .map(|y| VoxelCoord::new(voxel.x(), y, voxel.z())),
        voxel
            .z()
            .checked_add(1)
            .map(|z| VoxelCoord::new(voxel.x(), voxel.y(), z)),
        voxel
            .z()
            .checked_sub(1)
            .map(|z| VoxelCoord::new(voxel.x(), voxel.y(), z)),
    ]
    .into_iter()
    .flatten()
}

/// Sound travels along actual face-connected, empty voxels; a short bend is
/// audible even when the ordinary two-cell visual ray is blocked. No unloaded
/// or opaque voxel is treated as free air. Every search is capped at five steps
/// and cannot discover a remote region or leak a blocked source location.
pub(super) fn can_hear_junk_rig(
    registries: &Registries,
    state: &AppState,
    rig: KoboldJunkRig,
    listener: VoxelCoord,
) -> bool {
    audible_path(registries, state, rig.position(), listener, rig.kind())
}

fn audible_path(
    registries: &Registries,
    state: &AppState,
    source: VoxelCoord,
    listener: VoxelCoord,
    kind: KoboldJunkRigKind,
) -> bool {
    let reach = acoustic_reach(kind);
    let direct_distance = source
        .x()
        .abs_diff(listener.x())
        .saturating_add(source.y().abs_diff(listener.y()))
        .saturating_add(source.z().abs_diff(listener.z()));
    if direct_distance > u64::from(reach) {
        return false;
    }
    // The authored opening graph has no voxel authority until the world is
    // generated. Keep the pre-generation listener bounded and local.
    if state.world().generation_profile().is_none() {
        return true;
    }
    let passable = |voxel| {
        !state.structures().blocks_full_cell(voxel)
            && state
                .world()
                .block_at(voxel)
                .and_then(|id| registries.world().block_for_state(id))
                .is_some_and(|block| block.collision() == BlockCollisionShape::Empty)
    };
    if !passable(source) || !passable(listener) {
        return false;
    }
    let mut visited = BTreeSet::from([source]);
    let mut queue = VecDeque::from([(source, 0_u8)]);
    while let Some((current, distance)) = queue.pop_front() {
        if current == listener {
            return true;
        }
        if distance == reach {
            continue;
        }
        for neighbor in neighbors(current) {
            if visited.insert(neighbor) && passable(neighbor) {
                queue.push_back((neighbor, distance + 1));
            }
        }
    }
    false
}

#[cfg(test)]
#[path = "acoustics_tests.rs"]
mod tests;
