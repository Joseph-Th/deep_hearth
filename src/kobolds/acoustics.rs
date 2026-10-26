//! Bounded local percussion through actual open voxels, independently of sight.
//!
//! A sound may follow a bent air corridor but not cut across solid rock, built panels, or an
//! unknown/unloaded chunk. With no generated terrain, the opening wilderness
//! uses only a short Manhattan-radius approximation. This is not a world-wide
//! noise map or a signal network; rig transmission has separate physical rules.

use std::collections::{BTreeSet, VecDeque};

use crate::core::state::AppState;
use crate::registry::Registries;
use crate::spatial::{CubeFace, VoxelCoord};
use crate::world::BlockCollisionShape;

use super::junkworks::{KoboldJunkRig, KoboldJunkRigKind};
use super::visibility::within_manhattan_reach;

fn acoustic_reach(kind: KoboldJunkRigKind) -> u8 {
    match kind {
        // The wooden brace dampens the crack; a bare stone clapper carries farther.
        KoboldJunkRigKind::TimberRatchet => 3,
        KoboldJunkRigKind::StoneClatter => 5,
        // Shaped iron rings clearly beyond the radius of a raw stone clapper.
        KoboldJunkRigKind::IronChime => 6,
    }
}

fn neighbors(voxel: VoxelCoord) -> impl Iterator<Item = VoxelCoord> {
    [
        CubeFace::East,
        CubeFace::West,
        CubeFace::Top,
        CubeFace::Bottom,
        CubeFace::South,
        CubeFace::North,
    ]
    .into_iter()
    .filter_map(move |face| face.adjacent_voxel(voxel))
}

/// Sound travels along actual face-connected, empty voxels; a short bend is
/// audible even when the ordinary two-cell visual ray is blocked. No unloaded
/// or opaque voxel is treated as free air. Every search is capped at six steps
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
    if !within_manhattan_reach(source, listener, u64::from(reach)) {
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
