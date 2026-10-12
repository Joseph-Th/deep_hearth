//! Pure deterministic layout and variation calculations for the bounded opening wilderness trail.

use crate::core::quantity::Mass;
use crate::spatial::VoxelCoord;

use super::{INITIAL_WILDERNESS_TRAIL_STEPS, WorldGenerationSeed};

const OPENING_TRAIL_DIRECTION_SALT: u64 = 0x5452_4149_4C5F_4449;
pub(super) const OPENING_RESOURCE_ORDER_SALT: u64 = 0x5245_534F_5552_4345;

fn mix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

fn horizontal_step(position: VoxelCoord, direction: usize) -> Option<VoxelCoord> {
    match direction {
        0 => position
            .x()
            .checked_add(1)
            .map(|x| VoxelCoord::new(x, position.y(), position.z())),
        1 => position
            .z()
            .checked_add(1)
            .map(|z| VoxelCoord::new(position.x(), position.y(), z)),
        2 => position
            .x()
            .checked_sub(1)
            .map(|x| VoxelCoord::new(x, position.y(), position.z())),
        3 => position
            .z()
            .checked_sub(1)
            .map(|z| VoxelCoord::new(position.x(), position.y(), z)),
        _ => unreachable!("horizontal direction index is bounded modulo four"),
    }
}

fn direction_supports_opening_trail(spawn: VoxelCoord, direction: usize) -> bool {
    let mut position = spawn;
    for _ in 0..INITIAL_WILDERNESS_TRAIL_STEPS {
        let Some(next) = horizontal_step(position, direction) else {
            return false;
        };
        position = next;
    }
    true
}

pub(super) fn generated_opening_trail(
    seed: WorldGenerationSeed,
    spawn: VoxelCoord,
) -> [VoxelCoord; INITIAL_WILDERNESS_TRAIL_STEPS + 1] {
    let first = usize::try_from(local_entropy(seed, spawn, OPENING_TRAIL_DIRECTION_SALT) % 4)
        .unwrap_or_else(|_| unreachable!("bounded trail direction fits usize"));
    let direction = (0..4)
        .map(|offset| (first + offset) % 4)
        .find(|direction| direction_supports_opening_trail(spawn, *direction))
        .unwrap_or_else(|| {
            unreachable!("every finite voxel supports four horizontal steps in some direction")
        });
    let mut trail = [spawn; INITIAL_WILDERNESS_TRAIL_STEPS + 1];
    for index in 1..trail.len() {
        trail[index] = horizontal_step(trail[index - 1], direction)
            .unwrap_or_else(|| unreachable!("selected trail direction was prevalidated"));
    }
    trail
}

pub(super) fn local_entropy(seed: WorldGenerationSeed, spawn: VoxelCoord, salt: u64) -> u64 {
    mix64(
        seed.value()
            ^ (spawn.x() as u64).wrapping_mul(0xD1B5_4A32_D192_ED03)
            ^ (spawn.y() as u64).wrapping_mul(0x94D0_49BB_1331_11EB)
            ^ (spawn.z() as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9)
            ^ salt,
    )
}

pub(super) fn generated_mass(
    seed: WorldGenerationSeed,
    spawn: VoxelCoord,
    salt: u64,
    base_mg: u64,
    variation_mg: u64,
) -> Mass {
    let width = variation_mg
        .checked_add(1)
        .unwrap_or_else(|| unreachable!("bounded wilderness variation width fits u64"));
    Mass::from_milligrams(base_mg + local_entropy(seed, spawn, salt) % width)
}
