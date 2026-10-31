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

use super::terrain::{mix64, terrain_height};
use super::{BlockStateId, HabitatKind, WorldChunk, WorldGenerationProfile, habitat_at};

pub(super) fn seeded_spring_spec(
    state: &AppState,
    profile: WorldGenerationProfile,
    chunk: &WorldChunk,
    forage_sites: &[VoxelCoord],
) -> Option<(VoxelCoord, GeneratedFluidStoreSpec)> {
    if profile.solid_state() != BLOCK_STATE_STONE {
        return None;
    }
    let coord = chunk.coord();
    let hash = mix64(
        profile.seed().value()
            ^ (coord.x().div_euclid(2) as u64).wrapping_mul(0x7BCA_49C1_BB79_1AF3)
            ^ (coord.z().div_euclid(2) as u64).wrapping_mul(0xC685_7BF3_81B4_D1DF)
            ^ 0x5350_5249_4E47_535F,
    );
    if coord.x().rem_euclid(2) as u64 != hash & 1
        || coord.z().rem_euclid(2) as u64 != (hash >> 1) & 1
    {
        return None;
    }
    let min_x = coord.world_min_x()?;
    let min_z = coord.world_min_z()?;
    let base_x = min_x.checked_add(3 + ((hash >> 8) % 10) as i64)?;
    let base_z = min_z.checked_add(3 + ((hash >> 22) % 10) as i64)?;
    let tree = super::super::generated_tree(profile, chunk);
    // Prefer low ground among nearby valid candidates, so the resource
    // tends to sit in an actual shallow hollow rather than on a ridge.
    let mut best = None;
    for (dx, dz) in [(0, 0), (2, 0), (0, 2), (-2, 0), (0, -2)] {
        let x = base_x.checked_add(dx)?;
        let z = base_z.checked_add(dz)?;
        let y = terrain_height(profile.seed(), x, z);
        let floor = VoxelCoord::new(x, y, z);
        let position = VoxelCoord::new(x, y.checked_add(1)?, z);
        if chunk.block_at(floor) != BLOCK_STATE_STONE
            || chunk.block_at(position) != BlockStateId::AIR
            || chunk.block_at(VoxelCoord::new(x, y.checked_add(2)?, z)) != BlockStateId::AIR
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
            || forage_sites.contains(&position)
        {
            continue;
        }
        let habitat = habitat_at(profile.seed(), x, z);
        let capacity = match habitat {
            HabitatKind::DampGrove => 5_000_000 + (hash >> 40) % 5_000_000,
            HabitatKind::MixedWoodland => 1_200_000 + (hash >> 40) % 2_000_000,
            HabitatKind::DryScrub => continue,
        };
        let candidate = (y, position, capacity);
        if best.is_none_or(|current: (i64, VoxelCoord, u64)| candidate < current) {
            best = Some(candidate);
        }
    }
    let (_, position, capacity) = best?;
    let volume = Volume::from_microliters(capacity);
    Some((
        position,
        GeneratedFluidStoreSpec::new(
            volume,
            FLUID_WATER,
            volume,
            Temperature::from_millikelvin(293_150),
        ),
    ))
}

#[cfg(test)]
#[path = "springs_tests.rs"]
mod tests;
