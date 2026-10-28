//! Persistent standing woodland and lazy calendar-time growth.
//!
//! A tree is one physical trunk with a compact biological record, not one scheduled update
//! per leaf. Biomass is derived from elapsed world time, even across saves or distant chunks.

use serde::{Deserialize, Serialize};

use crate::core::quantity::Mass;
use crate::core::time::SimulationTick;
use crate::spatial::VoxelCoord;

use super::{BlockStateId, CHUNK_SIDE, WorldChunk, WorldGenerationProfile};

/// Biological time is independent of the abbreviated four-season game calendar.
/// 24,000 slow ticks represent 24 hours, and one biological year is 365 such days.
pub const BIOLOGICAL_YEAR_TICKS: u64 = 24_000 * 365;
const ANNUAL_WOOD_GROWTH_MG: u64 = 20_000_000;
const MAX_WOOD_MASS_MG: u64 = 1_600_000_000;

/// One rooted, living temperate broadleaf. Standing wood is owned by the world until felled.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TreeRecord {
    root: VoxelCoord,
    sprouted_at: SimulationTick,
    initial_wood: Mass,
}

impl TreeRecord {
    #[must_use]
    pub const fn root(self) -> VoxelCoord {
        self.root
    }

    #[must_use]
    pub const fn sprouted_at(self) -> SimulationTick {
        self.sprouted_at
    }

    #[must_use]
    pub const fn initial_wood(self) -> Mass {
        self.initial_wood
    }

    /// Exact whole-milligram wood increment from atmospheric assimilation, capped at maturity.
    /// O(1) work, with no per-tree appointments or per-leaf update queue.
    #[must_use]
    pub fn wood_at(self, now: SimulationTick) -> Mass {
        let elapsed = now.value().saturating_sub(self.sprouted_at.value());
        let growth = u128::from(elapsed).saturating_mul(u128::from(ANNUAL_WOOD_GROWTH_MG))
            / u128::from(BIOLOGICAL_YEAR_TICKS);
        let total =
            (u128::from(self.initial_wood.milligrams()) + growth).min(u128::from(MAX_WOOD_MASS_MG));
        Mass::from_milligrams(total as u64)
    }

    /// Physical trunk occupies two vertically adjacent cells; crown is visual only until
    /// foliage/canopy physics exists. Root occupancy does not depend on calendar tick.
    #[must_use]
    pub fn occupies_trunk(self, voxel: VoxelCoord) -> bool {
        voxel.x() == self.root.x()
            && voxel.z() == self.root.z()
            && (voxel.y() == self.root.y() || self.root.y().checked_add(1) == Some(voxel.y()))
    }

    #[must_use]
    pub fn canopy_radius_voxels(self, now: SimulationTick) -> u8 {
        match self.wood_at(now).milligrams() {
            0..10_000_000 => 0,
            10_000_000..150_000_000 => 1,
            150_000_000..600_000_000 => 2,
            _ => 3,
        }
    }

    pub(super) const fn new(root: VoxelCoord, initial_wood: Mass) -> Self {
        Self {
            root,
            sprouted_at: SimulationTick::ZERO,
            initial_wood,
        }
    }
}

/// One deterministic, sparse, root-supported woodland feature per standard stone chunk.
/// No global vegetation scan or neighbor-dependent random-stream order is needed.
pub(crate) fn generated_tree(
    profile: WorldGenerationProfile,
    chunk: &WorldChunk,
) -> Option<TreeRecord> {
    if profile.solid_state() != crate::content::BLOCK_STATE_STONE {
        return None;
    }
    let (x, z) = seeded_tree_column(profile, chunk.coord())?;
    let floor_y = super::generation::terrain_height(profile.seed(), x, z);
    let root_y = floor_y.checked_add(1)?;
    let root = VoxelCoord::new(x, root_y, z);
    if chunk.block_at(VoxelCoord::new(x, floor_y, z)) != profile.solid_state()
        || chunk.block_at(root) != BlockStateId::AIR
        || chunk.block_at(VoxelCoord::new(x, root_y.checked_add(1)?, z)) != BlockStateId::AIR
    {
        return None;
    }
    let mixed = tree_entropy(profile.seed(), chunk.coord());
    // A small minority are juvenile, while established trees supply meaningful timber.
    let initial = if mixed & 3 == 0 {
        8_000_000
    } else {
        280_000_000 + (mixed >> 24) % 180_000_000
    };
    Some(TreeRecord::new(root, Mass::from_milligrams(initial)))
}

fn tree_entropy(seed: super::WorldGenerationSeed, chunk: super::ChunkCoord) -> u64 {
    let value = seed.value()
        ^ (chunk.x() as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (chunk.z() as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    let value = (value ^ (value >> 30)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 27)
}

/// Position varies by seed and chunk, while retaining a three-voxel border so trees never
/// require neighboring chunks to be generated before their own trunk is authoritative.
pub(super) fn seeded_tree_column(
    profile: WorldGenerationProfile,
    chunk: super::ChunkCoord,
) -> Option<(i64, i64)> {
    if profile.solid_state() != crate::content::BLOCK_STATE_STONE {
        return None;
    }
    let mixed = tree_entropy(profile.seed(), chunk);
    let x = chunk
        .world_min_x()?
        .checked_add(3 + (mixed % (CHUNK_SIDE as u64 - 6)) as i64)?;
    let z = chunk
        .world_min_z()?
        .checked_add(3 + ((mixed >> 12) % (CHUNK_SIDE as u64 - 6)) as i64)?;
    Some((x, z))
}
