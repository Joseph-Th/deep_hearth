//! Generated mineral lenses are real finite world blocks with a valid ore path.

use super::*;

use std::collections::BTreeSet;

use crate::content::{
    BLOCK_STATE_CASSITERITE_ORE, BLOCK_STATE_HEMATITE_ORE, BLOCK_STATE_SPHALERITE_ORE,
    BLOCK_STATE_STONE, FORM_CRUSHED, FORM_ORE, MATERIAL_CASSITERITE, MATERIAL_HEMATITE,
    MATERIAL_SPHALERITE, PROCESS_CRUSH_ORE, build_registries,
};
use crate::core::state::{AppState, validate_loaded_state};
use crate::material::{CommodityKey, MaterialFormCohesion};
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::spatial::VoxelCoord;

#[test]
fn seeded_underground_lenses_include_three_chemically_distinct_minerals() {
    let registries = build_registries();
    let seed = WorldGenerationSeed::new(0x4D49_4E45_5241_4C53);
    let mut state = AppState::new();
    let mut found = BTreeSet::new();
    let mut first = None;
    for x in 0..4 {
        for z in 0..4 {
            let chunk = ChunkCoord::new(x, z);
            validate_generate_chunk(&registries, &state, seed, chunk, BLOCK_STATE_STONE)
                .unwrap()
                .commit(&mut state)
                .unwrap();
            let min_x = chunk.world_min_x().unwrap();
            let min_z = chunk.world_min_z().unwrap();
            for x in min_x..min_x + CHUNK_SIDE as i64 {
                for z in min_z..min_z + CHUNK_SIDE as i64 {
                    for y in -55..=-26 {
                        let point = VoxelCoord::new(x, y, z);
                        let block = state.world().block_at(point).unwrap();
                        if [
                            BLOCK_STATE_HEMATITE_ORE,
                            BLOCK_STATE_CASSITERITE_ORE,
                            BLOCK_STATE_SPHALERITE_ORE,
                        ]
                        .contains(&block)
                        {
                            first.get_or_insert((point, block));
                            found.insert(block);
                        }
                    }
                }
            }
        }
    }
    assert_eq!(
        found.len(),
        3,
        "all three minerals must occur in the sampled seeded region"
    );
    for (state_id, mineral, density) in [
        (BLOCK_STATE_HEMATITE_ORE, MATERIAL_HEMATITE, 5_260u64),
        (BLOCK_STATE_CASSITERITE_ORE, MATERIAL_CASSITERITE, 6_900),
        (BLOCK_STATE_SPHALERITE_ORE, MATERIAL_SPHALERITE, 4_100),
    ] {
        let block = registries.world().block_for_state(state_id).unwrap();
        assert_eq!(
            block.commodity(),
            Some(CommodityKey::new(mineral, FORM_ORE))
        );
        assert_eq!(
            block.excavation().unwrap().fragmented_commodity(),
            CommodityKey::new(mineral, FORM_ORE)
        );
        assert_eq!(block.represented_mass().milligrams(), density * 1_000_000);
        assert!(
            registries
                .materials()
                .has_commodity(CommodityKey::new(mineral, FORM_CRUSHED))
        );
        assert_eq!(
            registries
                .materials()
                .get_form(FORM_ORE)
                .unwrap()
                .cohesion(),
            MaterialFormCohesion::Loose
        );
    }
    assert_eq!(
        registries
            .ore_processing()
            .get_comminution(PROCESS_CRUSH_ORE)
            .unwrap()
            .input_form(),
        FORM_ORE
    );
    let sample = first.unwrap();
    assert_eq!(state.world().block_at(sample.0), Some(sample.1));
    assert!(calculate_matter_accounting(&state).is_ok());
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let save = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&save)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(loaded, state);
}

#[test]
fn mineral_generation_is_seeded_bounded_and_does_not_replace_shallow_rock() {
    let registries = build_registries();
    let seeds = [WorldGenerationSeed::new(37), WorldGenerationSeed::new(38)];
    let mut profiles = Vec::new();
    for seed in seeds {
        let mut state = AppState::new();
        validate_generate_chunk(
            &registries,
            &state,
            seed,
            ChunkCoord::new(0, 0),
            BLOCK_STATE_STONE,
        )
        .unwrap()
        .commit(&mut state)
        .unwrap();
        let mineral_voxels = (0..CHUNK_SIDE as i64)
            .flat_map(|x| {
                (0..CHUNK_SIDE as i64)
                    .flat_map(move |z| (-55..=-26).map(move |y| VoxelCoord::new(x, y, z)))
            })
            .filter(|&voxel| state.world().block_at(voxel) != Some(BLOCK_STATE_STONE))
            .filter(|&voxel| state.world().block_at(voxel) != Some(BlockStateId::AIR))
            .collect::<Vec<_>>();
        profiles.push(mineral_voxels);
        for x in 0..CHUNK_SIDE as i64 {
            for z in 0..CHUNK_SIDE as i64 {
                // The normal shallower stone geology remains unaffected.
                assert_eq!(
                    state.world().block_at(VoxelCoord::new(x, -24, z)),
                    Some(BLOCK_STATE_STONE)
                );
            }
        }
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    }
    assert_ne!(profiles[0], profiles[1]);
}
