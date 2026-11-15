//! Continuous, multi-scale seeded terrain shared by chunk construction, tree
//! identity, habitats, and natural water. Broad valleys and ridges change the
//! travel and shelter problem; hills and local roughness keep exploration varied.

use super::WorldGenerationSeed;

const TERRAIN_LATTICE_SIDE: i64 = 8;
const TERRAIN_HALF_RANGE: i64 = 4;
const HILLS_LATTICE_SIDE: i64 = 32;
const HILLS_HALF_RANGE: i64 = 8;
const REGION_LATTICE_SIDE: i64 = 64;
const REGION_HALF_RANGE: i64 = 14;
// The origin's bounded reference clearing remains the original 8 m terrain:
// maintained tutorial scripts have exact face-neighbor body routes there.
// Outside it, relief smoothly increases into genuinely varied natural land.
const LOCAL_SALT: u64 = 0;
const REFERENCE_CLEARING_RADIUS: u64 = 32;
const RELIEF_TRANSITION_WIDTH: u64 = 80;
const HILLS_SALT: u64 = 0x5445_5252_4149_3332;
const REGION_SALT: u64 = 0x5445_5252_4149_3634;
const LATTICE_CORNERS_PER_CHUNK: usize = super::CHUNK_SIDE / TERRAIN_LATTICE_SIDE as usize + 1;
const _: () = assert!(
    super::CHUNK_SIDE.is_multiple_of(TERRAIN_LATTICE_SIDE as usize),
    "chunk height sampling requires an exact whole number of terrain lattice cells"
);

pub(in crate::world) fn terrain_height(seed: WorldGenerationSeed, x: i64, z: i64) -> i64 {
    let detail = field_height(
        seed,
        x,
        z,
        TERRAIN_LATTICE_SIDE,
        TERRAIN_HALF_RANGE,
        LOCAL_SALT,
    );
    let regional_relief =
        field_height(
            seed,
            x,
            z,
            REGION_LATTICE_SIDE,
            REGION_HALF_RANGE,
            REGION_SALT,
        ) + field_height(seed, x, z, HILLS_LATTICE_SIDE, HILLS_HALF_RANGE, HILLS_SALT);
    detail + regional_relief * relief_strength(x, z) / RELIEF_TRANSITION_WIDTH as i64
}

fn relief_strength(x: i64, z: i64) -> i64 {
    let distance = x.unsigned_abs().max(z.unsigned_abs());
    distance
        .saturating_sub(REFERENCE_CLEARING_RADIUS)
        .min(RELIEF_TRANSITION_WIDTH) as i64
}

/// Sample one aligned generated chunk with each shared lattice corner hashed once.
///
/// Independent point lookups remain authoritative for trees, forage and habitat.
/// Chunk construction reuses shared lattice corners, including the broad 64 m
/// regional field, so adjacent chunks agree exactly at their boundary.
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
            std::array::from_fn(|x| {
                lattice_height(
                    seed,
                    base_x + x as i64,
                    base_z + z as i64,
                    TERRAIN_HALF_RANGE,
                    LOCAL_SALT,
                )
            })
        });
    let hills = chunk_field(
        seed,
        min_x,
        min_z,
        HILLS_LATTICE_SIDE,
        HILLS_HALF_RANGE,
        HILLS_SALT,
    );
    let region = chunk_field(
        seed,
        min_x,
        min_z,
        REGION_LATTICE_SIDE,
        REGION_HALF_RANGE,
        REGION_SALT,
    );
    let span = TERRAIN_LATTICE_SIDE as usize;
    std::array::from_fn(|local_z| {
        std::array::from_fn(|local_x| {
            let cell_x = local_x / span;
            let cell_z = local_z / span;
            let detail = interpolate_height(
                corners[cell_z][cell_x],
                corners[cell_z][cell_x + 1],
                corners[cell_z + 1][cell_x],
                corners[cell_z + 1][cell_x + 1],
                (local_x % span) as i64,
                (local_z % span) as i64,
                TERRAIN_LATTICE_SIDE,
            );
            let x = min_x + local_x as i64;
            let z = min_z + local_z as i64;
            let relief = hills[local_z][local_x] + region[local_z][local_x];
            detail + relief * relief_strength(x, z) / RELIEF_TRANSITION_WIDTH as i64
        })
    })
}

fn field_height(
    seed: WorldGenerationSeed,
    x: i64,
    z: i64,
    side: i64,
    half_range: i64,
    salt: u64,
) -> i64 {
    let cell_x = x.div_euclid(side);
    let cell_z = z.div_euclid(side);
    let corners = field_corners(seed, cell_x, cell_z, half_range, salt);
    interpolate_height(
        corners[0][0],
        corners[0][1],
        corners[1][0],
        corners[1][1],
        x.rem_euclid(side),
        z.rem_euclid(side),
        side,
    )
}

/// A 16 m chunk fits entirely in one 32 m or 64 m lattice cell, so only four
/// corners need hashing for each broad field.
fn chunk_field(
    seed: WorldGenerationSeed,
    min_x: i64,
    min_z: i64,
    side: i64,
    half_range: i64,
    salt: u64,
) -> [[i64; super::CHUNK_SIDE]; super::CHUNK_SIDE] {
    let corners = field_corners(
        seed,
        min_x.div_euclid(side),
        min_z.div_euclid(side),
        half_range,
        salt,
    );
    std::array::from_fn(|local_z| {
        std::array::from_fn(|local_x| {
            interpolate_height(
                corners[0][0],
                corners[0][1],
                corners[1][0],
                corners[1][1],
                min_x.rem_euclid(side) + local_x as i64,
                min_z.rem_euclid(side) + local_z as i64,
                side,
            )
        })
    })
}

fn field_corners(
    seed: WorldGenerationSeed,
    cell_x: i64,
    cell_z: i64,
    half_range: i64,
    salt: u64,
) -> [[i64; 2]; 2] {
    std::array::from_fn(|z| {
        std::array::from_fn(|x| {
            lattice_height(seed, cell_x + x as i64, cell_z + z as i64, half_range, salt)
        })
    })
}

fn interpolate_height(h00: i64, h10: i64, h01: i64, h11: i64, fx: i64, fz: i64, side: i64) -> i64 {
    let north = lerp_integer(h00, h10, fx, side);
    let south = lerp_integer(h01, h11, fx, side);
    lerp_integer(north, south, fz, side)
}

fn lattice_height(seed: WorldGenerationSeed, x: i64, z: i64, half_range: i64, salt: u64) -> i64 {
    let value = mix64(
        seed.value()
            ^ (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
            ^ (z as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
            ^ salt,
    );
    let width = u64::try_from(half_range * 2 + 1)
        .unwrap_or_else(|_| unreachable!("positive terrain height width fits u64"));
    i64::try_from(value % width).unwrap_or_else(|_| unreachable!("bounded terrain height fits i64"))
        - half_range
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
