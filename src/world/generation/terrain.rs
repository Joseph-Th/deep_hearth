//! Smoothed deterministic surface heights shared by chunk construction and tree identity.

use super::WorldGenerationSeed;

const TERRAIN_LATTICE_SIDE: i64 = 8;
const TERRAIN_HALF_RANGE: i64 = 4;

pub(in crate::world) fn terrain_height(seed: WorldGenerationSeed, x: i64, z: i64) -> i64 {
    let cell_x = x.div_euclid(TERRAIN_LATTICE_SIDE);
    let cell_z = z.div_euclid(TERRAIN_LATTICE_SIDE);
    let fx = x.rem_euclid(TERRAIN_LATTICE_SIDE);
    let fz = z.rem_euclid(TERRAIN_LATTICE_SIDE);
    let h00 = lattice_height(seed, cell_x, cell_z);
    let h10 = lattice_height(seed, cell_x + 1, cell_z);
    let h01 = lattice_height(seed, cell_x, cell_z + 1);
    let h11 = lattice_height(seed, cell_x + 1, cell_z + 1);
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

pub(super) fn mix64(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}
