//! Deterministic seeded mineral lenses and grade selection inside generated rock.

use super::terrain::mix64;
use super::{BlockStateId, ChunkCoord, VoxelCoord, WorldGenerationSeed, WorldRegistry};

const MINERAL_MIN_Y: i64 = -55;
const MINERAL_MAX_Y: i64 = -26;
const LENS_SIDE: usize = 4;
const LENS_COLUMNS: usize = super::CHUNK_SIDE / LENS_SIDE;
const LENS_MIN_Y: i64 = MINERAL_MIN_Y.div_euclid(LENS_SIDE as i64);
const LENS_LEVELS: usize = (MINERAL_MAX_Y.div_euclid(LENS_SIDE as i64) - LENS_MIN_Y + 1) as usize;
const _: () = assert!(
    super::CHUNK_SIDE.is_multiple_of(LENS_SIDE),
    "chunk mineral sampling requires whole lens cells"
);

/// Grade-resolved deep ore cells for one generated chunk, keyed by their
/// four-metre geological lattice rather than by individual excavatable voxels.
/// Every lookup within the authored vertical window resolves identical seeded
/// mineral identity without repeating thousands of hashes for the same lens.
pub(super) struct ChunkMineralLenses {
    cells: [[[Option<BlockStateId>; LENS_LEVELS]; LENS_COLUMNS]; LENS_COLUMNS],
}

/// One district's two-by-two shallow outcrop, if this chunk owns its site.
/// Every generated column in a chunk belongs to the same 32-metre district.
#[derive(Clone, Copy)]
pub(super) struct ChunkSurfaceLode {
    local_x: usize,
    local_z: usize,
    ore: BlockStateId,
}

impl ChunkSurfaceLode {
    #[must_use]
    pub(super) fn at(self, x: usize, z: usize) -> Option<BlockStateId> {
        ((self.local_x..self.local_x + 2).contains(&x)
            && (self.local_z..self.local_z + 2).contains(&z))
        .then_some(self.ore)
    }
}

/// Derive the single possible shallow outcrop per chunk with one district hash.
/// A separate pointwise seed-formula oracle verifies exact coordinates and
/// material identity without retaining per-voxel hashing in generation.
pub(super) fn seeded_surface_lode_for_chunk(
    seed: WorldGenerationSeed,
    chunk: ChunkCoord,
    states: [BlockStateId; 8],
) -> Option<ChunkSurfaceLode> {
    let hash = mix64(
        seed.value()
            ^ (chunk.x().div_euclid(2) as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
            ^ (chunk.z().div_euclid(2) as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
            ^ 0x5355_5246_4143_454F,
    );
    if chunk.x().rem_euclid(2) as u64 != hash & 1
        || chunk.z().rem_euclid(2) as u64 != (hash >> 1) & 1
    {
        return None;
    }
    Some(ChunkSurfaceLode {
        local_x: 3 + ((hash >> 8) % 10) as usize,
        local_z: 3 + ((hash >> 20) % 10) as usize,
        ore: states[((hash >> 36) % states.len() as u64) as usize],
    })
}

impl ChunkMineralLenses {
    pub(super) fn new(
        registry: &WorldRegistry,
        seed: WorldGenerationSeed,
        min_x: i64,
        min_z: i64,
        states: [BlockStateId; 8],
    ) -> Self {
        debug_assert_eq!(min_x.rem_euclid(super::CHUNK_SIDE as i64), 0);
        debug_assert_eq!(min_z.rem_euclid(super::CHUNK_SIDE as i64), 0);
        Self {
            cells: std::array::from_fn(|z| {
                std::array::from_fn(|x| {
                    std::array::from_fn(|level| {
                        // The first cell starts at -56, outside the legal ore
                        // range. Sample at -55 while retaining the same lattice
                        // identity and reject -56 at lookup time.
                        let y = ((LENS_MIN_Y + level as i64) * LENS_SIDE as i64).max(MINERAL_MIN_Y);
                        let position = VoxelCoord::new(
                            min_x + (x * LENS_SIDE) as i64,
                            y,
                            min_z + (z * LENS_SIDE) as i64,
                        );
                        seeded_mineral_state(seed, position, states)
                            .map(|ore| select_mineral_grade(registry, seed, position, ore))
                    })
                })
            }),
        }
    }

    #[must_use]
    pub(super) fn at(&self, local_x: usize, y: i64, local_z: usize) -> Option<BlockStateId> {
        if !(MINERAL_MIN_Y..=MINERAL_MAX_Y).contains(&y) {
            return None;
        }
        let level = (y.div_euclid(LENS_SIDE as i64) - LENS_MIN_Y) as usize;
        self.cells[local_z / LENS_SIDE][local_x / LENS_SIDE][level]
    }
}

/// Sparse surface expressions of finite shallow mineral bodies. The two-by-two
/// outcrop and the rock immediately below it have the same seeded identity.
/// Each 32 m district has one site in a seed-chosen 16 m chunk, so exploration
/// cannot leave entire neighboring districts barren by unlucky random rolls.
/// No neighbor-chunk generation or per-visit discovery roll moves the ore.
#[cfg(test)]
pub(super) fn seeded_surface_lode_state(
    seed: WorldGenerationSeed,
    x: i64,
    z: i64,
    states: [BlockStateId; 8],
) -> Option<BlockStateId> {
    seeded_surface_lode_for_chunk(
        seed,
        ChunkCoord::containing(VoxelCoord::new(x, 0, z)),
        states,
    )?
    .at(
        x.rem_euclid(super::CHUNK_SIDE as i64) as usize,
        z.rem_euclid(super::CHUNK_SIDE as i64) as usize,
    )
}

#[cfg(test)]
#[path = "minerals_tests.rs"]
mod tests;

/// 4 m mineral lenses at bounded deep elevations. This is seeded terrain
/// identity: no virtual deposits, no metal extracted from chemically bound ore,
/// and no repeated discovery rolls when the same chunk is reloaded.
pub(in crate::world) fn seeded_mineral_state(
    seed: WorldGenerationSeed,
    position: VoxelCoord,
    states: [BlockStateId; 8],
) -> Option<BlockStateId> {
    if !(MINERAL_MIN_Y..=MINERAL_MAX_Y).contains(&position.y()) {
        return None;
    }
    let hash = mix64(
        seed.value()
            ^ (position.x().div_euclid(4) as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
            ^ (position.y().div_euclid(4) as u64).wrapping_mul(0xD6E8_FEB8_6659_FD93)
            ^ (position.z().div_euclid(4) as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
            ^ 0x4D49_4E45_5241_4C53,
    );
    // 4 m lenses are sparse, and unlike a global mineral uniform roll,
    // their types reflect shallow vs deep host-rock position. Lattice hash
    // remains invariant under chunk and generation-order changes.
    hash.is_multiple_of(89).then(|| {
        let choice = ((hash >> 13) % 8) as usize;
        // Select strata using the lattice cell, not individual voxel elevation;
        // otherwise one 4 m lens crossing -40 splits into unrelated ores.
        let index = if position.y().div_euclid(4) >= -10 {
            // Upper deep rock: tin/zinc/lead/graphite/quartz plus iron oxide.
            [0, 1, 2, 4, 5, 6, 7, 7][choice]
        } else {
            // Lower deep rock: magnetite and sulfides dominate the rare lenses.
            [0, 0, 1, 2, 3, 3, 5, 5][choice]
        };
        states[index]
    })
}

pub(super) fn select_mineral_grade(
    registry: &WorldRegistry,
    seed: WorldGenerationSeed,
    position: VoxelCoord,
    mineral: BlockStateId,
) -> BlockStateId {
    let graded = match mineral {
        crate::content::BLOCK_STATE_HEMATITE_ORE => {
            Some(crate::content::BLOCK_STATE_GRADED_HEMATITE_ORE)
        }
        crate::content::BLOCK_STATE_CASSITERITE_ORE => {
            Some(crate::content::BLOCK_STATE_GRADED_CASSITERITE_ORE)
        }
        crate::content::BLOCK_STATE_MAGNETITE_ORE => {
            Some(crate::content::BLOCK_STATE_GRADED_MAGNETITE_ORE)
        }
        crate::content::BLOCK_STATE_GALENA_ORE => {
            Some(crate::content::BLOCK_STATE_GRADED_GALENA_ORE)
        }
        _ => None,
    };
    graded
        .filter(|&state| {
            registry.get_state(state).is_some() && seeded_mineral_is_graded(seed, position)
        })
        .unwrap_or(mineral)
}

/// A second seeded lattice decision chooses rich vs gangue-bearing material in the same
/// four-metre body, preserving the original mineral species across all grade variants.
fn seeded_mineral_is_graded(seed: WorldGenerationSeed, position: VoxelCoord) -> bool {
    let hash = mix64(
        seed.value()
            ^ (position.x().div_euclid(4) as u64).wrapping_mul(0xAA9C_3E12_6B65_8D21)
            ^ (position.y().div_euclid(4) as u64).wrapping_mul(0xCA12_4D09_3578_F6EB)
            ^ (position.z().div_euclid(4) as u64).wrapping_mul(0xE9C3_56B1_420A_DD75)
            ^ 0x4752_4144_4544_4F52,
    );
    hash & 1 == 1
}
