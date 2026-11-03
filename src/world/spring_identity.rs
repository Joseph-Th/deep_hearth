//! Immutable seed-derived spring locations and capacities, independent of later world changes.
//!
//! Generation filters these candidates through live terrain and occupancy. Trusted load
//! checks the immutable candidate envelope without replaying historical occupancy.

use crate::content::BLOCK_STATE_STONE;
use crate::core::quantity::Volume;
use crate::spatial::VoxelCoord;

use super::generation::WorldGenerationProfile;
use super::generation::habitat_at;
use super::generation::terrain::{mix64, terrain_height};
use super::{ChunkCoord, HabitatKind};

/// The same candidate geometry and volume drives generation and persisted identity checks.
pub(super) fn seeded_spring_candidates(
    profile: WorldGenerationProfile,
    coord: ChunkCoord,
) -> impl Iterator<Item = (VoxelCoord, Volume)> {
    let origin = (|| {
        if profile.solid_state() != BLOCK_STATE_STONE {
            return None;
        }
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
        Some((hash, base_x, base_z))
    })();
    [(0, 0), (2, 0), (0, 2), (-2, 0), (0, -2)]
        .into_iter()
        .filter_map(move |(dx, dz)| {
            let (hash, base_x, base_z) = origin?;
            let x = base_x.checked_add(dx)?;
            let z = base_z.checked_add(dz)?;
            let y = terrain_height(profile.seed(), x, z).checked_add(1)?;
            let capacity = match habitat_at(profile.seed(), x, z) {
                HabitatKind::DampGrove => 5_000_000 + (hash >> 40) % 5_000_000,
                HabitatKind::MixedWoodland => 1_200_000 + (hash >> 40) % 2_000_000,
                HabitatKind::DryScrub => return None,
            };
            Some((VoxelCoord::new(x, y, z), Volume::from_microliters(capacity)))
        })
}

/// Seed identity and capacity cannot change when water drains or terrain is excavated.
/// This is necessary but not sufficient for generation: historical occupancy can suppress a site.
pub(crate) fn seeded_spring_matches(
    profile: WorldGenerationProfile,
    position: VoxelCoord,
    capacity: Volume,
) -> bool {
    seeded_spring_candidates(profile, ChunkCoord::containing(position))
        .any(|(candidate, expected)| candidate == position && expected == capacity)
}
