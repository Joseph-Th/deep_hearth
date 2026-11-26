//! Fresh-world spawn selection over real generated ground and seeded habitat.
//!
//! This is world-creation logic, not an actor observation or a second player
//! admission path. The player owner still validates and commits the chosen site.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::content::BLOCK_STATE_STONE;
use crate::core::state::AppState;
use crate::logistics::{PlayerBodyState, body_has_fixed_placement_floor};
use crate::player::{PlayerAdmissionState, player_admission_state, validate_admit_player};
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::world::{BlockStateId, CHUNK_SIDE, ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y};

use super::habitat::{HabitatKind, habitat_at};
use super::terrain::mix64;
use super::{
    ChunkGenerationCommitError, ChunkGenerationError, WorldGenerationSeed,
    validate_generate_natural_chunk,
};

#[derive(Debug)]
pub enum NaturalWorldStartError {
    Generation(ChunkGenerationError),
    Commit(ChunkGenerationCommitError),
    NoSafeSite,
}

impl Display for NaturalWorldStartError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Generation(error) => write!(formatter, "natural terrain generation failed: {error}"),
            Self::Commit(error) => write!(formatter, "natural terrain commit failed: {error}"),
            Self::NoSafeSite => formatter.write_str(
                "no safely occupiable natural terrain in 64 seeded districts matching the requested habitat",
            ),
        }
    }
}

impl Error for NaturalWorldStartError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Generation(error) => Some(error),
            Self::Commit(error) => Some(error),
            Self::NoSafeSite => None,
        }
    }
}

/// Create the first natural district and select a safe starting cell. Rejected
/// candidates never enter the returned world. Adapters admit the player and
/// expand residency separately; this operation grants no carried resources.
pub fn generate_natural_world_start(
    registries: &Registries,
    seed: WorldGenerationSeed,
    desired: Option<HabitatKind>,
) -> Result<(AppState, VoxelCoord), NaturalWorldStartError> {
    let hash = |value: u64| mix64(value.wrapping_add(0x9E37_79B9_7F4A_7C15));
    for attempt in 0..64_u64 {
        let salt = seed.value() ^ attempt.wrapping_mul(0xD1B5_4A32_D192_ED03);
        let x = hash(salt ^ 0x4E41_5455_5241_4C58);
        let z = hash(salt ^ 0x4E41_5455_5241_4C5A);
        let chunk = ChunkCoord::new((x % 49) as i64 - 24, (z % 49) as i64 - 24);
        let mut state = AppState::new();
        validate_generate_natural_chunk(registries, &state, seed, chunk, BLOCK_STATE_STONE)
            .map_err(NaturalWorldStartError::Generation)?
            .commit(&mut state)
            .map_err(NaturalWorldStartError::Commit)?;
        let position_hash = hash(salt ^ 0x4E41_5455_5241_4C50);
        if let Some(spawn) =
            select_natural_world_spawn(registries, &state, chunk, position_hash, desired)
        {
            return Ok((state, spawn));
        }
    }
    Err(NaturalWorldStartError::NoSafeSite)
}

/// Prefer a physically admissible site with a two-step level walking route,
/// falling back to a single exit where the generated hillside offers no such
/// route. The odd stride visits each column once; habitat remains per-site.
#[must_use]
fn select_natural_world_spawn(
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
            if !is_grounded_spawn_site(registries, state, foot) {
                continue;
            }
            match level_route(registries, state, chunk, foot) {
                LevelRoute::Continuing => return Some(foot),
                LevelRoute::SingleExit => {
                    single_exit_fallback.get_or_insert(foot);
                }
                LevelRoute::NoExit => {}
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

fn level_neighbors(chunk: ChunkCoord, foot: VoxelCoord) -> impl Iterator<Item = VoxelCoord> {
    [(1, 0), (-1, 0), (0, 1), (0, -1)]
        .into_iter()
        .filter_map(move |(dx, dz)| {
            let neighbor = VoxelCoord::new(
                foot.x().checked_add(dx)?,
                foot.y(),
                foot.z().checked_add(dz)?,
            );
            (ChunkCoord::containing(neighbor) == chunk).then_some(neighbor)
        })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LevelRoute {
    NoExit,
    SingleExit,
    Continuing,
}

/// A first valid neighbor is not necessarily a traversable starting area.
/// Prefer an exit with a second supported, body-clear standing cell, allowing
/// a turn but not counting the spawn itself as forward progress. Only the
/// already generated startup chunk is consulted; this cannot predict a route
/// through undiscovered terrain or manufacture flat ground.
fn level_route(
    registries: &Registries,
    state: &AppState,
    chunk: ChunkCoord,
    foot: VoxelCoord,
) -> LevelRoute {
    let mut route = LevelRoute::NoExit;
    for neighbor in level_neighbors(chunk, foot) {
        if !is_level_walking_exit(registries, state, neighbor) {
            continue;
        }
        route = LevelRoute::SingleExit;
        if level_neighbors(chunk, neighbor)
            .any(|next| next != foot && is_level_walking_exit(registries, state, next))
        {
            return LevelRoute::Continuing;
        }
    }
    route
}

/// A neighbor can be supported by ore or built full-cell matter as well as
/// stone. Read the actual body-footprint grounding kernel instead of treating
/// the spawn's natural-stone preference as a walking rule. A moving resident
/// cannot provide a dependable walking exit for a newly admitted player.
fn is_level_walking_exit(registries: &Registries, state: &AppState, at: VoxelCoord) -> bool {
    validate_admit_player(registries, state, at).is_ok()
        && PlayerBodyState::centered_on_voxel_floor(at)
            .is_some_and(|body| body_has_fixed_placement_floor(registries, state, body))
}

#[cfg(test)]
#[path = "spawn_tests.rs"]
mod tests;
