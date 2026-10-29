//! Bounded local percussion through actual open voxels, independently of sight.
//!
//! A sound may follow a bent air corridor but not cut across solid rock, built panels, or an
//! unknown/unloaded chunk along a generated route. In ungenerated opening space,
//! even when distant chunks exist, the opening uses a local
//! Manhattan bound when unobstructed, or a bounded air detour around real built
//! walls. This is not a world-wide noise map or signal network; rig transmission
//! has separate physical rules.

use std::collections::{BTreeSet, VecDeque};

use crate::core::state::AppState;
use crate::registry::Registries;
use crate::spatial::{CubeFace, VoxelCoord};
use crate::world::{BlockCollisionShape, ChunkCoord};

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
/// audible even when the ordinary two-cell visual ray is blocked. Opaque
/// terrain always obstructs; missing terrain fails closed on generated local
/// routes and follows the authored opening graph elsewhere. Search is capped at six steps
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
    // The authored graph has no terrain opacity. Built full-cell members
    // still block sound, and may lengthen its physical path beyond the
    // authored hearing radius. Use the indexed structural broadphase before
    // deciding whether a bounded air-corridor search is necessary.
    let generated_at_endpoint = [source, listener]
        .into_iter()
        .any(|position| state.world().has_generated_terrain_at(position));
    let passable = |voxel| {
        !state.structures().blocks_full_cell(voxel)
            && state.world().tree_trunk_at(voxel).is_none()
            && match state.world().block_at(voxel) {
                Some(id) => registries
                    .world()
                    .block_for_state(id)
                    .is_some_and(|block| block.collision() == BlockCollisionShape::Empty),
                None => !generated_at_endpoint,
            }
    };
    if !passable(source) || !passable(listener) {
        return false;
    }
    if !generated_at_endpoint
        && ChunkCoord::containing(source) == ChunkCoord::containing(listener)
        && !state
            .structures()
            .has_full_cell_member_in_voxel_box(source, listener)
    {
        return true;
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
