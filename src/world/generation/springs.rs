//! Sparse, finite surface water pockets in actual moisture-favoured terrain.
//!
//! Each two-by-two chunk district has one independent candidate. Wet hollows
//! are worth seeking; dry scrub has no generated standing water. The volume
//! belongs to the existing fluid owner, is consumed by ordinary drinking,
//! and never respawns when the player revisits a generated chunk.

use crate::content::{BLOCK_STATE_STONE, FLUID_WATER};
use crate::core::quantity::{Temperature, Volume};
use crate::core::state::AppState;
use crate::fluid::GeneratedFluidStoreSpec;
use crate::spatial::VoxelCoord;

use super::super::spring_identity::seeded_spring_candidates;
use super::{BlockStateId, WorldChunk, WorldGenerationProfile};

pub(super) fn seeded_spring_spec(
    state: &AppState,
    profile: WorldGenerationProfile,
    chunk: &WorldChunk,
    forage_sites: &[VoxelCoord],
) -> Option<(VoxelCoord, GeneratedFluidStoreSpec)> {
    let tree = super::super::generated_tree(profile, chunk);
    // Prefer low ground among nearby valid candidates, so the resource
    // tends to sit in an actual shallow hollow rather than on a ridge.
    let mut best = None;
    for (position, capacity) in seeded_spring_candidates(profile, chunk.coord()) {
        let floor = VoxelCoord::new(position.x(), position.y().checked_sub(1)?, position.z());
        let upper = VoxelCoord::new(position.x(), position.y().checked_add(1)?, position.z());
        if chunk.block_at(floor) != BLOCK_STATE_STONE
            || chunk.block_at(position) != BlockStateId::AIR
            || chunk.block_at(upper) != BlockStateId::AIR
            || tree.is_some_and(|tree| tree.occupies_trunk(position))
            || state.world().tree_trunk_at(position).is_some()
            || state.structures().blocks_full_cell(position)
            || state.logistics().has_stationary_occupant_at(position)
            || state.logistics().fluid_stores_at(position).next().is_some()
            || state
                .surface()
                .available_resources_at(Some(position))
                .next()
                .is_some()
            // Historical depletion does not remove living roots. Natural
            // water cannot be introduced in the same physical plant cell.
            || state.surface().rooted_biomass_occupies_voxel(position)
            || forage_sites.contains(&position)
        {
            continue;
        }
        let candidate = (floor.y(), position, capacity);
        if best.is_none_or(|current: (i64, VoxelCoord, Volume)| candidate < current) {
            best = Some(candidate);
        }
    }
    let (_, position, capacity) = best?;
    Some((
        position,
        GeneratedFluidStoreSpec::new(
            capacity,
            FLUID_WATER,
            capacity,
            Temperature::from_millikelvin(293_150),
        ),
    ))
}

#[cfg(test)]
#[path = "springs_tests.rs"]
mod tests;
