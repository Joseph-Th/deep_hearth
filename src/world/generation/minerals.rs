//! Seeded geological provinces with sparse outcrops and mineral lenses.
//!
//! One 128 m province biases the nearby 32 m outcrop districts and the
//! underground 4 m lens cells toward a shared geology. Prospecting can find
//! productive belts or nearly barren rock; no explored district is owed ore.

use super::terrain::mix64;
use super::{BlockStateId, ChunkCoord, VoxelCoord, WorldGenerationSeed, WorldRegistry};

const MINERAL_MIN_Y: i64 = -55;
const MINERAL_MAX_Y: i64 = -26;
const LENS_SIDE: usize = 4;
const LENS_COLUMNS: usize = super::CHUNK_SIDE / LENS_SIDE;
const LENS_MIN_Y: i64 = MINERAL_MIN_Y.div_euclid(LENS_SIDE as i64);
const LENS_LEVELS: usize = (MINERAL_MAX_Y.div_euclid(LENS_SIDE as i64) - LENS_MIN_Y + 1) as usize;
const PROVINCE_SIDE: i64 = 128;
const PROVINCE_SALT: u64 = 0x4745_4F4C_4F47_5950;
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

/// Regional rock context, not a new inventory material or a player-visible
/// survey result. All contained blocks remain independently real and finite.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GeologicalProvince {
    IronBelt,
    TinUplands,
    SulfideBasin,
    PoorBasement,
}

fn geological_province(seed: WorldGenerationSeed, x: i64, z: i64) -> GeologicalProvince {
    geological_province_cell(
        seed,
        x.div_euclid(PROVINCE_SIDE),
        z.div_euclid(PROVINCE_SIDE),
    )
}

fn geological_province_cell(
    seed: WorldGenerationSeed,
    cell_x: i64,
    cell_z: i64,
) -> GeologicalProvince {
    let hash = mix64(
        seed.value()
            ^ (cell_x as u64).wrapping_mul(0xB529_7A4D_0FCF_0B5D)
            ^ (cell_z as u64).wrapping_mul(0x68E3_1DA4_8B27_51A9)
            ^ PROVINCE_SALT,
    );
    match hash & 3 {
        0 => GeologicalProvince::IronBelt,
        1 => GeologicalProvince::TinUplands,
        2 => GeologicalProvince::SulfideBasin,
        _ => GeologicalProvince::PoorBasement,
    }
}

fn outcrop_present(province: GeologicalProvince, hash: u64) -> bool {
    let roll = (hash >> 48) & 63;
    roll < match province {
        GeologicalProvince::IronBelt => 48,
        GeologicalProvince::TinUplands => 32,
        GeologicalProvince::SulfideBasin => 24,
        GeologicalProvince::PoorBasement => 2,
    }
}

fn mineral_index(province: GeologicalProvince, choice: usize, upper: bool) -> usize {
    let choices = match (province, upper) {
        (GeologicalProvince::IronBelt, true) => [0, 0, 0, 3, 3, 6, 7, 7],
        (GeologicalProvince::IronBelt, false) => [0, 0, 3, 3, 5, 5, 0, 3],
        (GeologicalProvince::TinUplands, true) => [1, 1, 1, 1, 7, 7, 2, 0],
        (GeologicalProvince::TinUplands, false) => [1, 1, 1, 5, 5, 3, 0, 3],
        (GeologicalProvince::SulfideBasin, true) => [2, 2, 4, 4, 5, 5, 7, 1],
        (GeologicalProvince::SulfideBasin, false) => [2, 4, 4, 5, 5, 3, 3, 0],
        (GeologicalProvince::PoorBasement, true) => [6, 6, 7, 7, 7, 7, 0, 1],
        (GeologicalProvince::PoorBasement, false) => [0, 0, 3, 3, 3, 0, 5, 1],
    };
    choices[choice]
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
    let province = geological_province_cell(seed, chunk.x().div_euclid(8), chunk.z().div_euclid(8));
    if !outcrop_present(province, hash) {
        return None;
    }
    if chunk.x().rem_euclid(2) as u64 != hash & 1
        || chunk.z().rem_euclid(2) as u64 != (hash >> 1) & 1
    {
        return None;
    }
    Some(ChunkSurfaceLode {
        local_x: 3 + ((hash >> 8) % 10) as usize,
        local_z: 3 + ((hash >> 20) % 10) as usize,
        ore: states[mineral_index(province, ((hash >> 36) % 8) as usize, true)],
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
        let province = geological_province(seed, min_x, min_z);
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
                        seeded_mineral_state_in_province(seed, position, states, province)
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
/// Each 32 m district has at most one site in a seed-chosen 16 m chunk.
/// The 128 m geological province biases its presence and mineral species.
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
#[cfg(test)]
pub(in crate::world) fn seeded_mineral_state(
    seed: WorldGenerationSeed,
    position: VoxelCoord,
    states: [BlockStateId; 8],
) -> Option<BlockStateId> {
    seeded_mineral_state_in_province(
        seed,
        position,
        states,
        geological_province(seed, position.x(), position.z()),
    )
}

fn seeded_mineral_state_in_province(
    seed: WorldGenerationSeed,
    position: VoxelCoord,
    states: [BlockStateId; 8],
    province: GeologicalProvince,
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
    // Broad provinces change mineral frequency and chemistry. Individual
    // lenses remain 4 m bodies, not an ore reward for digging any location.
    let chance = match province {
        GeologicalProvince::IronBelt => 55,
        GeologicalProvince::TinUplands => 67,
        GeologicalProvince::SulfideBasin => 43,
        GeologicalProvince::PoorBasement => 293,
    };
    hash.is_multiple_of(chance).then(|| {
        let choice = ((hash >> 13) % 8) as usize;
        // Select strata using the lattice cell, not individual voxel elevation;
        // otherwise one 4 m lens crossing -40 splits into unrelated ores.
        states[mineral_index(province, choice, position.y().div_euclid(4) >= -10)]
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
