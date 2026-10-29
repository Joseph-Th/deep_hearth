//! Deterministic seeded mineral lenses and grade selection inside generated rock.

use super::terrain::mix64;
use super::{BlockStateId, VoxelCoord, WorldGenerationSeed, WorldRegistry};

/// 4 m mineral lenses at bounded deep elevations. This is seeded terrain
/// identity: no virtual deposits, no metal extracted from chemically bound ore,
/// and no repeated discovery rolls when the same chunk is reloaded.
pub(in crate::world) fn seeded_mineral_state(
    seed: WorldGenerationSeed,
    position: VoxelCoord,
    states: [BlockStateId; 8],
) -> Option<BlockStateId> {
    if !(-55..=-26).contains(&position.y()) {
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
