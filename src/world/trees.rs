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
const MAX_WOOD_MASS_MG: u64 = 1_600_000_000;
/// Dry-wood accumulation across juvenile, sapling, pole and mature phases.
/// Larger crowns assimilate more carbon until mature growth slows.
const GROWTH_STAGES: [(u64, u64); 4] = [
    (10_000_000, 4_000_000),
    (150_000_000, 18_000_000),
    (600_000_000, 40_000_000),
    (MAX_WOOD_MASS_MG, 22_000_000),
];
/// Common multiple of all four annual growth rates. One scaled time unit is a
/// fractional simulation tick, so each phase transition can be represented
/// exactly without rounding a fraction of a tick away.
const GROWTH_TIME_UNITS_PER_TICK: u128 = 3_960_000_000;
const _: () = {
    let mut index = 0;
    while index < GROWTH_STAGES.len() {
        assert!(
            GROWTH_TIME_UNITS_PER_TICK % GROWTH_STAGES[index].1 as u128 == 0,
            "every growth stage must divide the exact time scale"
        );
        index += 1;
    }
};

/// One rooted, living temperate broadleaf. Standing wood is owned by the world until felled.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TreeRecord {
    root: VoxelCoord,
    /// The generated seed site remains the tree's auditable biological identity after relocation.
    origin: VoxelCoord,
    sprouted_at: SimulationTick,
    initial_wood: Mass,
}

/// One settled felling event. Seed-derived identity makes its biomass receipt auditable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FelledTreeRecord {
    root: VoxelCoord,
    origin: VoxelCoord,
    felled_at: SimulationTick,
}

/// Immutable relocation evidence. Each original seed identity has one ordered
/// path from its natural root through every subsequent juvenile transplant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TransplantedTreeRecord {
    origin: VoxelCoord,
    from: VoxelCoord,
    to: VoxelCoord,
    transplanted_at: SimulationTick,
}

impl TransplantedTreeRecord {
    #[must_use]
    pub(crate) const fn new(
        origin: VoxelCoord,
        from: VoxelCoord,
        to: VoxelCoord,
        transplanted_at: SimulationTick,
    ) -> Self {
        Self {
            origin,
            from,
            to,
            transplanted_at,
        }
    }

    #[must_use]
    pub(crate) const fn origin(self) -> VoxelCoord {
        self.origin
    }
    #[must_use]
    pub(crate) const fn from(self) -> VoxelCoord {
        self.from
    }
    #[must_use]
    pub(crate) const fn to(self) -> VoxelCoord {
        self.to
    }
    #[must_use]
    pub(crate) const fn transplanted_at(self) -> SimulationTick {
        self.transplanted_at
    }
}

impl FelledTreeRecord {
    #[must_use]
    pub(crate) const fn root(self) -> VoxelCoord {
        self.root
    }

    #[must_use]
    pub(crate) const fn felled_at(self) -> SimulationTick {
        self.felled_at
    }

    pub(super) const fn new(tree: TreeRecord, felled_at: SimulationTick) -> Self {
        Self {
            root: tree.root,
            origin: tree.origin,
            felled_at,
        }
    }

    #[must_use]
    pub(crate) const fn origin_root(self) -> VoxelCoord {
        self.origin
    }
}

impl TreeRecord {
    #[must_use]
    pub const fn root(self) -> VoxelCoord {
        self.root
    }

    /// Seed-verified natural origin, unchanged by transplanting the young tree.
    #[must_use]
    pub const fn origin_root(self) -> VoxelCoord {
        self.origin
    }

    pub(super) fn transplanted_to(self, destination: VoxelCoord) -> Self {
        Self {
            root: destination,
            origin: self.origin,
            sprouted_at: self.sprouted_at,
            initial_wood: self.initial_wood,
        }
    }

    #[must_use]
    pub const fn sprouted_at(self) -> SimulationTick {
        self.sprouted_at
    }

    #[must_use]
    pub const fn initial_wood(self) -> Mass {
        self.initial_wood
    }

    /// Exact whole-milligram wood accumulation across four biological phases.
    /// Age is resolved analytically in constant time; no per-tree scheduled updates.
    #[must_use]
    pub fn wood_at(self, now: SimulationTick) -> Mass {
        let mut remaining_time = u128::from(now.value().saturating_sub(self.sprouted_at.value()))
            * GROWTH_TIME_UNITS_PER_TICK;
        let mut mass = u128::from(self.initial_wood.milligrams());
        for (ceiling, annual_growth) in GROWTH_STAGES {
            let ceiling = u128::from(ceiling);
            if mass >= ceiling {
                continue;
            }
            let annual_growth = u128::from(annual_growth);
            let required = ceiling - mass;
            // This scale is divisible by every authored stage's annual rate:
            // the elapsed time to each exact biomass threshold is integral
            // in scaled ticks, even when it falls between simulation ticks.
            let time_to_ceiling = required
                * u128::from(BIOLOGICAL_YEAR_TICKS)
                * (GROWTH_TIME_UNITS_PER_TICK / annual_growth);
            if remaining_time < time_to_ceiling {
                mass += annual_growth * remaining_time
                    / (u128::from(BIOLOGICAL_YEAR_TICKS) * GROWTH_TIME_UNITS_PER_TICK);
                break;
            }
            mass = ceiling;
            remaining_time -= time_to_ceiling;
        }
        Mass::from_milligrams(mass.min(u128::from(MAX_WOOD_MASS_MG)) as u64)
    }

    /// Physical trunk occupies two vertically adjacent cells. Crown now
    /// attenuates direct daylight, but is not a collision or gas-volume body.
    /// Root occupancy does not depend on calendar tick.
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
            origin: root,
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
    let tree = seeded_tree_identity(profile, chunk.coord())?;
    let root = tree.root();
    let floor_y = root.y().checked_sub(1)?;
    let (x, z) = (root.x(), root.z());
    if chunk.block_at(VoxelCoord::new(x, floor_y, z)) != profile.solid_state()
        || chunk.block_at(root) != BlockStateId::AIR
        || chunk.block_at(VoxelCoord::new(x, root.y().checked_add(1)?, z)) != BlockStateId::AIR
    {
        return None;
    }
    Some(tree)
}

/// The immutable seed-specific identity of the sole natural tree site in this chunk.
/// An occupied site may have prevented tree generation, but a decoded tree may never
/// claim a different origin, juvenile stage, mass or sprouting timestamp.
pub(super) fn seeded_tree_identity(
    profile: WorldGenerationProfile,
    chunk: super::ChunkCoord,
) -> Option<TreeRecord> {
    let (x, z) = seeded_tree_column(profile, chunk)?;
    let root_y = super::generation::terrain_height(profile.seed(), x, z).checked_add(1)?;
    let mixed = tree_entropy(profile.seed(), chunk);
    // A small minority are juvenile, while established trees supply meaningful timber.
    let initial = if mixed & 3 == 0 {
        8_000_000
    } else {
        280_000_000 + (mixed >> 24) % 180_000_000
    };
    Some(TreeRecord::new(
        VoxelCoord::new(x, root_y, z),
        Mass::from_milligrams(initial),
    ))
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
