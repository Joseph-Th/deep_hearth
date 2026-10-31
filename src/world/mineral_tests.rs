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

#[test]
fn generated_shallow_ore_is_a_visible_finite_and_persistent_discovery() {
    let registries = build_registries();
    let seed = WorldGenerationSeed::new(0x5355_5246_4143_454F);
    let mut state = AppState::new();
    let mut found = None;
    'chunks: for cx in 0..5 {
        for cz in 0..5 {
            let chunk = ChunkCoord::new(cx, cz);
            validate_generate_chunk(&registries, &state, seed, chunk, BLOCK_STATE_STONE)
                .unwrap()
                .commit(&mut state)
                .unwrap();
            for x in chunk.world_min_x().unwrap()..chunk.world_min_x().unwrap() + CHUNK_SIDE as i64
            {
                for z in
                    chunk.world_min_z().unwrap()..chunk.world_min_z().unwrap() + CHUNK_SIDE as i64
                {
                    let y = super::generation::terrain_height(seed, x, z);
                    let surface = VoxelCoord::new(x, y, z);
                    let block = state.world().block_at(surface).unwrap();
                    if let Some(ore) = registries
                        .world()
                        .block_for_state(block)
                        .and_then(|definition| definition.commodity())
                        && ore.form() == FORM_ORE
                    {
                        for depth in 0..4 {
                            let below = VoxelCoord::new(x, y - depth, z);
                            let below_block = state.world().block_at(below).unwrap();
                            let definition =
                                registries.world().block_for_state(below_block).unwrap();
                            assert_eq!(definition.commodity(), Some(ore));
                            assert_eq!(
                                definition.excavation().unwrap().fragmented_commodity(),
                                ore
                            );
                        }
                        found = Some((surface, block));
                        break 'chunks;
                    }
                }
            }
        }
    }
    let (surface, ore) = found.expect("regional terrain must provide a discoverable outcrop");
    // The actual player can point at the lode from its surface, without
    // querying underground coordinates or a scenario-specific resource list.
    crate::player::validate_admit_player(
        &registries,
        &state,
        VoxelCoord::new(surface.x(), surface.y() + 1, surface.z()),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let downward = crate::spatial::ViewDirection::new(0, -1_000_000, 0).unwrap();
    assert_eq!(
        crate::world::raycast_player_block(&registries, &state, downward)
            .unwrap()
            .map(|target| target.voxel()),
        Some(surface)
    );
    assert!(calculate_matter_accounting(&state).is_ok());
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let save = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&save)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(loaded, state);
    assert_eq!(loaded.world().block_at(surface), Some(ore));
}

#[test]
fn additional_minerals_have_conserved_ore_forms_and_stratified_seeded_sites() {
    let registries = build_registries();
    let states = [
        BLOCK_STATE_HEMATITE_ORE,
        BLOCK_STATE_CASSITERITE_ORE,
        BLOCK_STATE_SPHALERITE_ORE,
        crate::content::BLOCK_STATE_MAGNETITE_ORE,
        crate::content::BLOCK_STATE_GALENA_ORE,
        crate::content::BLOCK_STATE_PYRITE_ORE,
        crate::content::BLOCK_STATE_GRAPHITE_ORE,
        crate::content::BLOCK_STATE_QUARTZ_ORE,
    ];
    let facts = [
        (crate::content::MATERIAL_HEMATITE, 5_260_u64),
        (crate::content::MATERIAL_CASSITERITE, 6_900),
        (crate::content::MATERIAL_SPHALERITE, 4_100),
        (crate::content::MATERIAL_MAGNETITE, 5_150),
        (crate::content::MATERIAL_GALENA, 7_600),
        (crate::content::MATERIAL_PYRITE, 5_010),
        (crate::content::MATERIAL_GRAPHITE, 2_260),
        (crate::content::MATERIAL_QUARTZ, 2_650),
    ];
    for (block_id, (material, density)) in states.into_iter().zip(facts) {
        let block = registries.world().block_for_state(block_id).unwrap();
        assert_eq!(
            block.commodity(),
            Some(CommodityKey::new(material, FORM_ORE))
        );
        assert_eq!(
            block.excavation().unwrap().fragmented_commodity(),
            CommodityKey::new(material, FORM_ORE)
        );
        assert_eq!(block.represented_mass().milligrams(), density * 1_000_000);
        assert!(
            registries
                .materials()
                .has_commodity(CommodityKey::new(material, FORM_CRUSHED))
        );
        let appearance = registries
            .textures()
            .get_commodity_appearance(CommodityKey::new(material, FORM_ORE))
            .unwrap();
        assert!(appearance.block().is_some() && appearance.object().is_some());
        let crushed = registries
            .textures()
            .get_commodity_appearance(CommodityKey::new(material, FORM_CRUSHED))
            .unwrap();
        assert!(crushed.object().is_some());
    }
    let seed = WorldGenerationSeed::new(0x4D49_4E45_5241_4C53);
    let mut seen = BTreeSet::new();
    let mut upper = BTreeSet::new();
    let mut lower = BTreeSet::new();
    for x in 0..100 {
        for z in 0..100 {
            assert_eq!(
                super::generation::seeded_mineral_state(
                    seed,
                    VoxelCoord::new(x * 4, -40, z * 4),
                    states
                ),
                super::generation::seeded_mineral_state(
                    seed,
                    VoxelCoord::new(x * 4, -39, z * 4),
                    states
                ),
                "the -40 m strata boundary must not divide one 4 m mineral lens",
            );
            for &y in &[-27, -31, -35, -39, -43, -47, -51, -55] {
                let point = VoxelCoord::new(x * 4, y, z * 4);
                let selected = super::generation::seeded_mineral_state(seed, point, states);
                if let Some(block) = selected {
                    seen.insert(block);
                    if y > -40 {
                        upper.insert(block);
                    } else {
                        lower.insert(block);
                    }
                }
            }
        }
    }
    assert_eq!(seen.len(), states.len());
    assert!(upper.contains(&crate::content::BLOCK_STATE_GRAPHITE_ORE));
    assert!(upper.contains(&crate::content::BLOCK_STATE_QUARTZ_ORE));
    assert!(!lower.contains(&crate::content::BLOCK_STATE_GRAPHITE_ORE));
    assert!(!lower.contains(&crate::content::BLOCK_STATE_QUARTZ_ORE));
    assert!(lower.contains(&crate::content::BLOCK_STATE_MAGNETITE_ORE));
    assert!(lower.contains(&crate::content::BLOCK_STATE_PYRITE_ORE));
}
