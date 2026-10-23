//! Seeded, grade-bearing lenses bridge compact terrain generation and ore beneficiation.

use super::*;
use std::collections::BTreeSet;

use crate::content::{
    BLOCK_STATE_CASSITERITE_ORE, BLOCK_STATE_GALENA_ORE, BLOCK_STATE_GRADED_CASSITERITE_ORE,
    BLOCK_STATE_GRADED_GALENA_ORE, BLOCK_STATE_GRADED_MAGNETITE_ORE, BLOCK_STATE_MAGNETITE_ORE,
    BLOCK_STATE_STONE, MATERIAL_CASSITERITE, MATERIAL_GALENA, MATERIAL_MAGNETITE, MATERIAL_STONE,
    build_registries,
};
use crate::core::state::{AppState, validate_loaded_state};
use crate::spatial::VoxelCoord;

#[test]
fn seeded_lenses_have_recoverable_gangue_at_density_correct_material_mass() {
    let registries = build_registries();
    let seed = WorldGenerationSeed::new(0x4D49_4E45_5241_4C53);
    let variants = [
        (
            BLOCK_STATE_GRADED_CASSITERITE_ORE,
            MATERIAL_CASSITERITE,
            BLOCK_STATE_CASSITERITE_ORE,
            600_000,
        ),
        (
            BLOCK_STATE_GRADED_MAGNETITE_ORE,
            MATERIAL_MAGNETITE,
            BLOCK_STATE_MAGNETITE_ORE,
            600_000,
        ),
        (
            BLOCK_STATE_GRADED_GALENA_ORE,
            MATERIAL_GALENA,
            BLOCK_STATE_GALENA_ORE,
            200_000,
        ),
    ];
    let mut state = AppState::new();
    let mut seen = BTreeSet::new();
    for x in 0..5 {
        for z in 0..5 {
            validate_generate_chunk(
                &registries,
                &state,
                seed,
                ChunkCoord::new(x, z),
                BLOCK_STATE_STONE,
            )
            .unwrap()
            .commit(&mut state)
            .unwrap();
            for local_x in 0..CHUNK_SIDE as i64 {
                for local_z in 0..CHUNK_SIDE as i64 {
                    for y in -55..=-26 {
                        let voxel = VoxelCoord::new(
                            x * CHUNK_SIDE as i64 + local_x,
                            y,
                            z * CHUNK_SIDE as i64 + local_z,
                        );
                        seen.insert(state.world().block_at(voxel).unwrap());
                    }
                }
            }
        }
    }
    for (graded, material, pure, gangue_ppm) in variants {
        assert!(
            seen.contains(&pure),
            "seed should retain rich mineral lenses"
        );
        assert!(
            seen.contains(&graded),
            "seed must create grade-bearing mineral lenses"
        );
        let graded_block = registries.world().block_for_state(graded).unwrap();
        let grade = graded_block.excavation().unwrap().composition();
        assert_eq!(grade.parts_per_million(material), 1_000_000 - gangue_ppm);
        assert_eq!(grade.parts_per_million(MATERIAL_STONE), gangue_ppm);
        assert_eq!(
            graded_block.represented_mass(),
            mixed_full_cube_mass(registries.materials(), material, MATERIAL_STONE, gangue_ppm),
        );
        let mass = graded_block.represented_mass().milligrams();
        let pure_mass = registries
            .world()
            .block_for_state(pure)
            .unwrap()
            .represented_mass()
            .milligrams();
        let stone_mass = registries
            .world()
            .block_for_state(BLOCK_STATE_STONE)
            .unwrap()
            .represented_mass()
            .milligrams();
        assert!(stone_mass < mass && mass < pure_mass);
    }
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
