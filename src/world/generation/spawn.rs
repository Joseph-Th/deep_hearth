//! Fresh-world spawn selection over real generated ground and seeded habitat.
//!
//! This is world-creation logic, not an actor observation or a second player
//! admission path. The player owner still validates and commits the chosen site.

use crate::content::BLOCK_STATE_STONE;
use crate::core::state::AppState;
use crate::logistics::{PlayerBodyState, player_body_is_grounded_at};
use crate::player::{PlayerAdmissionState, player_admission_state, validate_admit_player};
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::world::{BlockStateId, CHUNK_SIDE, ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y};

use super::habitat::{HabitatKind, habitat_at};

/// Prefer a physically admissible site with a two-step level walking route,
/// falling back to a single exit where the generated hillside offers no such
/// route. The odd stride visits each column once; habitat remains per-site.
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
    let mut single_exit_fallback = None;
    for n in 0..columns {
        let index = (start + n * stride) % columns;
        let x = min_x.checked_add((index % CHUNK_SIDE) as i64)?;
        let z = min_z.checked_add((index / CHUNK_SIDE) as i64)?;
        if desired.is_some_and(|kind| habitat_at(seed, x, z) != kind) {
            continue;
        }
        for y in (WORLD_MIN_Y + 1..WORLD_MAX_Y_EXCLUSIVE - 2).rev() {
            let foot = VoxelCoord::new(x, y, z);
            if is_grounded_spawn_site(registries, state, foot)
                && has_level_walking_exit(registries, state, chunk, foot)
            {
                if has_continuing_level_route(registries, state, chunk, foot) {
                    return Some(foot);
                }
                single_exit_fallback.get_or_insert(foot);
            }
        }
    }
    single_exit_fallback
}

/// One physical standing-site test for both the starting cell and its exit.
/// Use ordinary player admission, not a separate tree/structure collision model.
fn is_grounded_spawn_site(registries: &Registries, state: &AppState, foot: VoxelCoord) -> bool {
    let (Some(head_y), Some(floor_y)) = (foot.y().checked_add(1), foot.y().checked_sub(1)) else {
        return false;
    };
    state.world().block_at(foot) == Some(BlockStateId::AIR)
        && state
            .world()
            .block_at(VoxelCoord::new(foot.x(), head_y, foot.z()))
            == Some(BlockStateId::AIR)
        && state
            .world()
            .block_at(VoxelCoord::new(foot.x(), floor_y, foot.z()))
            == Some(BLOCK_STATE_STONE)
        && validate_admit_player(registries, state, foot).is_ok()
}

/// Before admitting a new player, only the initial chunk has collision truth.
/// Require one walkable neighbor inside that chunk. The startup neighborhood
/// expansion then cannot turn an isolated cliff-top start into a forced jump
/// or fall on the player's very first move.
fn has_level_walking_exit(
    registries: &Registries,
    state: &AppState,
    chunk: ChunkCoord,
    foot: VoxelCoord,
) -> bool {
    [(1, 0), (-1, 0), (0, 1), (0, -1)]
        .into_iter()
        .filter_map(|(dx, dz)| {
            let neighbor = VoxelCoord::new(
                foot.x().checked_add(dx)?,
                foot.y(),
                foot.z().checked_add(dz)?,
            );
            (ChunkCoord::containing(neighbor) == chunk).then_some(neighbor)
        })
        .any(|neighbor| is_level_walking_exit(registries, state, neighbor))
}

/// A first valid neighbor is not necessarily a traversable starting area.
/// Prefer an exit with a second supported, body-clear standing cell, allowing
/// a turn but not counting the spawn itself as forward progress. Only the
/// already generated startup chunk is consulted; this cannot predict a route
/// through undiscovered terrain or manufacture flat ground.
fn has_continuing_level_route(
    registries: &Registries,
    state: &AppState,
    chunk: ChunkCoord,
    foot: VoxelCoord,
) -> bool {
    const DIRECTIONS: [(i64, i64); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
    DIRECTIONS.into_iter().any(|(dx, dz)| {
        let Some((x, z)) = foot.x().checked_add(dx).zip(foot.z().checked_add(dz)) else {
            return false;
        };
        let neighbor = VoxelCoord::new(x, foot.y(), z);
        if ChunkCoord::containing(neighbor) != chunk
            || !is_level_walking_exit(registries, state, neighbor)
        {
            return false;
        }
        DIRECTIONS.into_iter().any(|(next_dx, next_dz)| {
            let Some((next_x, next_z)) = x.checked_add(next_dx).zip(z.checked_add(next_dz)) else {
                return false;
            };
            let next = VoxelCoord::new(next_x, foot.y(), next_z);
            next != foot
                && ChunkCoord::containing(next) == chunk
                && is_level_walking_exit(registries, state, next)
        })
    })
}

/// A neighbor can be supported by ore or built full-cell matter as well as
/// stone. Read the actual body-footprint grounding kernel instead of treating
/// the spawn's natural-stone preference as a walking rule.
fn is_level_walking_exit(registries: &Registries, state: &AppState, at: VoxelCoord) -> bool {
    validate_admit_player(registries, state, at).is_ok()
        && PlayerBodyState::centered_on_voxel_floor(at)
            .is_some_and(|body| player_body_is_grounded_at(registries, state, body))
}

#[cfg(test)]
#[path = "spawn_tests.rs"]
mod tests;
