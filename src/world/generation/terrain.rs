//! Smoothed deterministic surface heights shared by chunk construction and tree identity.

use super::WorldGenerationSeed;

const TERRAIN_LATTICE_SIDE: i64 = 8;
const TERRAIN_HALF_RANGE: i64 = 4;
const LATTICE_CORNERS_PER_CHUNK: usize = super::CHUNK_SIDE / TERRAIN_LATTICE_SIDE as usize + 1;
const _: () = assert!(
    super::CHUNK_SIDE.is_multiple_of(TERRAIN_LATTICE_SIDE as usize),
    "chunk height sampling requires an exact whole number of terrain lattice cells"
);

pub(in crate::world) fn terrain_height(seed: WorldGenerationSeed, x: i64, z: i64) -> i64 {
    let cell_x = x.div_euclid(TERRAIN_LATTICE_SIDE);
    let cell_z = z.div_euclid(TERRAIN_LATTICE_SIDE);
    let fx = x.rem_euclid(TERRAIN_LATTICE_SIDE);
    let fz = z.rem_euclid(TERRAIN_LATTICE_SIDE);
    let h00 = lattice_height(seed, cell_x, cell_z);
    let h10 = lattice_height(seed, cell_x + 1, cell_z);
    let h01 = lattice_height(seed, cell_x, cell_z + 1);
    let h11 = lattice_height(seed, cell_x + 1, cell_z + 1);
    interpolate_height(h00, h10, h01, h11, fx, fz)
}

/// Sample one aligned generated chunk with each shared lattice corner hashed once.
///
/// Independent point lookups remain authoritative for trees, forage and habitat.
/// Chunk construction has 256 columns but only nine distinct noise corners:
/// reusing them avoids repeating four hashes per column while preserving the
/// same integer interpolation, including Euclidean rounding at negative heights.
pub(super) fn terrain_heights_for_chunk(
    seed: WorldGenerationSeed,
    min_x: i64,
    min_z: i64,
) -> [[i64; super::CHUNK_SIDE]; super::CHUNK_SIDE] {
    debug_assert_eq!(min_x.rem_euclid(super::CHUNK_SIDE as i64), 0);
    debug_assert_eq!(min_z.rem_euclid(super::CHUNK_SIDE as i64), 0);
    let base_x = min_x.div_euclid(TERRAIN_LATTICE_SIDE);
    let base_z = min_z.div_euclid(TERRAIN_LATTICE_SIDE);
    let corners: [[i64; LATTICE_CORNERS_PER_CHUNK]; LATTICE_CORNERS_PER_CHUNK] =
        std::array::from_fn(|z| {
            std::array::from_fn(|x| lattice_height(seed, base_x + x as i64, base_z + z as i64))
        });
    let span = TERRAIN_LATTICE_SIDE as usize;
    std::array::from_fn(|local_z| {
        std::array::from_fn(|local_x| {
            let cell_x = local_x / span;
            let cell_z = local_z / span;
            interpolate_height(
                corners[cell_z][cell_x],
                corners[cell_z][cell_x + 1],
                corners[cell_z + 1][cell_x],
                corners[cell_z + 1][cell_x + 1],
                (local_x % span) as i64,
                (local_z % span) as i64,
            )
        })
    })
}

fn interpolate_height(h00: i64, h10: i64, h01: i64, h11: i64, fx: i64, fz: i64) -> i64 {
    let north = lerp_integer(h00, h10, fx, TERRAIN_LATTICE_SIDE);
    let south = lerp_integer(h01, h11, fx, TERRAIN_LATTICE_SIDE);
    lerp_integer(north, south, fz, TERRAIN_LATTICE_SIDE)
}

fn lattice_height(seed: WorldGenerationSeed, x: i64, z: i64) -> i64 {
    let value = mix64(
        seed.value()
            ^ (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
            ^ (z as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F),
    );
    let width = u64::try_from(TERRAIN_HALF_RANGE * 2 + 1)
        .unwrap_or_else(|_| unreachable!("positive terrain height width fits u64"));
    i64::try_from(value % width).unwrap_or_else(|_| unreachable!("bounded terrain height fits i64"))
        - TERRAIN_HALF_RANGE
}

fn lerp_integer(left: i64, right: i64, offset: i64, span: i64) -> i64 {
    (left * (span - offset) + right * offset).div_euclid(span)
}

pub(in crate::world) fn mix64(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

#[cfg(test)]
#[path = "terrain_tests.rs"]
mod tests;
