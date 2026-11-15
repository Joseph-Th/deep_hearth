//! Fresh-world spawn selection over real generated ground and seeded habitat.
//!
//! This is world-creation logic, not an actor observation or a second player
//! admission path. The player owner still validates and commits the chosen site.

use crate::content::BLOCK_STATE_STONE;
use crate::core::state::AppState;
use crate::player::{PlayerAdmissionState, player_admission_state, validate_admit_player};
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::world::{BlockStateId, CHUNK_SIDE, ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y};

use super::habitat::{HabitatKind, habitat_at};

/// Finds the first physically admissible site in the requested habitat, if any.
/// The odd stride visits each column once, keeping unfiltered natural starts
/// unchanged. Habitat is checked *per candidate*, not after choosing one
/// arbitrary safe site for the entire chunk.
#[must_use]
pub fn select_natural_world_spawn(
    registries: &Registries,
    state: &AppState,
    chunk: ChunkCoord,
    position_hash: u64,
    desired: Option<HabitatKind>,
) -> Option<VoxelCoord> {
    // World-creation preference must not become a remote habitat oracle for an
    // existing actor or a way around a partially initialized player.
    if player_admission_state(state) != PlayerAdmissionState::Absent {
        return None;
    }
    let seed = state.world().generation_profile()?.seed();
    let min_x = chunk.world_min_x()?;
    let min_z = chunk.world_min_z()?;
    let columns = CHUNK_SIDE * CHUNK_SIDE;
    let start = (position_hash & 255) as usize;
    let stride = (((position_hash >> 8) & 127) as usize) * 2 + 1;
    for n in 0..columns {
        let index = (start + n * stride) % columns;
        let x = min_x.checked_add((index % CHUNK_SIDE) as i64)?;
        let z = min_z.checked_add((index / CHUNK_SIDE) as i64)?;
        if desired.is_some_and(|kind| habitat_at(seed, x, z) != kind) {
            continue;
        }
        for y in (WORLD_MIN_Y + 1..WORLD_MAX_Y_EXCLUSIVE - 2).rev() {
            let foot = VoxelCoord::new(x, y, z);
            if state.world().block_at(foot) == Some(BlockStateId::AIR)
                && state.world().block_at(VoxelCoord::new(x, y + 1, z)) == Some(BlockStateId::AIR)
                && state.world().block_at(VoxelCoord::new(x, y - 1, z)) == Some(BLOCK_STATE_STONE)
                && validate_admit_player(registries, state, foot).is_ok()
            {
                return Some(foot);
            }
        }
    }
    None
}

#[cfg(test)]
#[path = "spawn_tests.rs"]
mod tests;
