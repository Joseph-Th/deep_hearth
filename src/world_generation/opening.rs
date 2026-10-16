//! Pure deterministic layout and variation calculations for the bounded opening wilderness graph.

use crate::core::quantity::{Mass, Volume};
use crate::spatial::VoxelCoord;

use crate::world::WorldGenerationSeed;

const OPENING_SPINE_DIRECTION_SALT: u64 = 0x5452_4149_4C5F_4449;
const OPENING_BRANCH_DIRECTION_SALT: u64 = 0x4252_414E_4348_4449;
pub(super) const OPENING_RESOURCE_ORDER_SALT: u64 = 0x5245_534F_5552_4345;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct GeneratedOpeningLayout {
    pub(super) spawn: VoxelCoord,
    pub(super) forage: VoxelCoord,
    pub(super) junction: VoxelCoord,
    pub(super) water: VoxelCoord,
    pub(super) material_forward: VoxelCoord,
    pub(super) material_branch: VoxelCoord,
}

impl GeneratedOpeningLayout {
    pub(super) fn edge_pairs(self) -> [(VoxelCoord, VoxelCoord); 5] {
        [
            (self.spawn, self.forage),
            (self.forage, self.junction),
            (self.forage, self.water),
            (self.junction, self.material_forward),
            (self.junction, self.material_branch),
        ]
    }

    pub(super) fn positions(self) -> [VoxelCoord; 6] {
        [
            self.spawn,
            self.forage,
            self.junction,
            self.water,
            self.material_forward,
            self.material_branch,
        ]
    }
}

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

fn direction_supports_opening_spine(spawn: VoxelCoord, direction: usize) -> bool {
    let mut position = spawn;
    for _ in 0..3 {
        let Some(next) = horizontal_step(position, direction) else {
            return false;
        };
        position = next;
    }
    true
}

fn perpendicular_directions(direction: usize) -> [usize; 2] {
    match direction {
        0 | 2 => [1, 3],
        1 | 3 => [0, 2],
        _ => unreachable!("horizontal direction index is bounded modulo four"),
    }
}

fn ordered_branch_directions(
    seed: WorldGenerationSeed,
    spawn: VoxelCoord,
    main_direction: usize,
) -> [usize; 2] {
    let candidates = perpendicular_directions(main_direction);
    let first = usize::try_from(local_entropy(seed, spawn, OPENING_BRANCH_DIRECTION_SALT) % 2)
        .unwrap_or_else(|_| unreachable!("bounded branch direction fits usize"));
    [candidates[first], candidates[(first + 1) % 2]]
}

fn opening_layout_for_directions(
    spawn: VoxelCoord,
    main_direction: usize,
    branch: usize,
) -> Option<GeneratedOpeningLayout> {
    let forage = horizontal_step(spawn, main_direction)?;
    let junction = horizontal_step(forage, main_direction)?;
    let material_forward = horizontal_step(junction, main_direction)?;
    let water = horizontal_step(forage, branch)?;
    let material_branch = horizontal_step(junction, branch)?;
    Some(GeneratedOpeningLayout {
        spawn,
        forage,
        junction,
        water,
        material_forward,
        material_branch,
    })
}

pub(super) fn generated_opening_layout_matching(
    seed: WorldGenerationSeed,
    spawn: VoxelCoord,
    mut accepts_position: impl FnMut(VoxelCoord) -> bool,
) -> Option<GeneratedOpeningLayout> {
    let first = usize::try_from(local_entropy(seed, spawn, OPENING_SPINE_DIRECTION_SALT) % 4)
        .unwrap_or_else(|_| unreachable!("bounded spine direction fits usize"));
    for main_direction in (0..4).map(|offset| (first + offset) % 4) {
        if !direction_supports_opening_spine(spawn, main_direction) {
            continue;
        }
        for branch in ordered_branch_directions(seed, spawn, main_direction) {
            let Some(layout) = opening_layout_for_directions(spawn, main_direction, branch) else {
                continue;
            };
            if layout.positions().into_iter().all(&mut accepts_position) {
                return Some(layout);
            }
        }
    }
    None
}

pub(super) fn generated_opening_layout(
    seed: WorldGenerationSeed,
    spawn: VoxelCoord,
) -> GeneratedOpeningLayout {
    generated_opening_layout_matching(seed, spawn, |_| true).unwrap_or_else(|| {
        unreachable!("every finite voxel supports a bounded horizontal opening layout")
    })
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
    let variation = local_entropy(seed, spawn, salt) % width;
    Mass::from_milligrams(
        base_mg
            .checked_add(variation)
            .unwrap_or_else(|| panic!("wilderness generated mass overflowed u64")),
    )
}

pub(super) fn generated_volume(
    seed: WorldGenerationSeed,
    spawn: VoxelCoord,
    salt: u64,
    base_ul: u64,
    variation_ul: u64,
) -> Volume {
    let width = variation_ul
        .checked_add(1)
        .unwrap_or_else(|| unreachable!("bounded wilderness volume variation width fits u64"));
    let variation = local_entropy(seed, spawn, salt) % width;
    Volume::from_microliters(
        base_ul
            .checked_add(variation)
            .unwrap_or_else(|| panic!("wilderness generated volume overflowed u64")),
    )
}
