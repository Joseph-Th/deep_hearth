//! Natural resource availability must vary, remain physical, and replay.

use super::*;
use crate::content::{BLOCK_STATE_STONE, FORM_FOOD, build_registries};
use crate::core::state::{AppState, validate_loaded_state};
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::simulation::advance_tick;
use crate::surface::{
    player_surface_gathering_options, validate_start_player_surface_gathering_option,
};
use crate::world::{
    WorldGenerationSeed, validate_generate_chunk, validate_generate_player_neighborhood,
};
use std::collections::BTreeSet;

#[test]
fn natural_districts_expose_all_four_materials_without_guaranteeing_any_one_at_spawn() {
    let registries = build_registries();
    let mut materials = BTreeSet::new();
    let mut missing = 0;
    let mut occupied = 0;
    let mut seeds_with_sources = 0;
    for seed_value in 0..28_u64 {
        let seed = WorldGenerationSeed::new(seed_value);
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
        let spawn = VoxelCoord::new(0, terrain_height(seed, 0, 0) + 1, 0);
        validate_admit_player(&registries, &state, spawn)
            .unwrap()
            .commit(&mut state)
            .unwrap();
        let before = calculate_matter_accounting(&state).unwrap().total();
        validate_generate_player_neighborhood(&registries, &state)
            .unwrap()
            .unwrap()
            .commit(&mut state)
            .unwrap();
        let mut current = 0;
        for record in state.surface().resources() {
            if record.commodity().form() == FORM_FOOD {
                continue;
            }
            current += 1;
            materials.insert(record.commodity());
            assert_eq!(record.kind(), SurfaceResourceKind::Opportunity);
            assert_eq!(
                record.lifecycle(),
                crate::surface::SurfaceResourceLifecycle::Available
            );
            assert_eq!(record.created_at(), crate::core::time::SimulationTick::ZERO);
            assert_eq!(
                state.world().block_at(record.position()),
                Some(BlockStateId::AIR)
            );
            let floor = VoxelCoord::new(
                record.position().x(),
                record.position().y() - 1,
                record.position().z(),
            );
            assert_eq!(state.world().block_at(floor), Some(BLOCK_STATE_STONE));
            assert!(record.remaining_mass() > Mass::ZERO);
        }
        if current == 0 {
            missing += 1;
        } else {
            occupied += current;
            seeds_with_sources += 1;
        }
        assert!(calculate_matter_accounting(&state).unwrap().total() > before);
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
        let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
        let restored = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
            .unwrap()
            .into_state(&registries)
            .unwrap();
        assert_eq!(
            restored, state,
            "seed {seed_value} must persist natural raw materials"
        );
    }
    assert_eq!(
        materials,
        BTreeSet::from([
            CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
            CommodityKey::new(MATERIAL_FLINT, FORM_LUMP),
            CommodityKey::new(MATERIAL_CLAY, FORM_LUMP),
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        ]),
        "random natural worlds must expose all primitive raw material paths"
    );
    assert!(seeds_with_sources >= 20 && occupied >= 30);
    assert!(
        missing <= 10,
        "some worlds may be sparse, not universally empty"
    );
}

#[test]
fn seeded_district_sites_are_distinct_at_signed_chunk_boundaries() {
    let profile =
        WorldGenerationProfile::new(WorldGenerationSeed::new(0xD15A_9A19), BLOCK_STATE_STONE);
    let mut seen = BTreeSet::new();
    for x in -12..12 {
        for z in -12..12 {
            let chunk = ChunkCoord::new(x, z);
            if let Some((_, world_x, world_z)) = district_site(profile, chunk) {
                assert_eq!(
                    ChunkCoord::containing(VoxelCoord::new(world_x, 0, world_z)),
                    chunk
                );
                assert!(seen.insert((world_x, world_z)));
            }
        }
    }
    assert_eq!(
        seen.len(),
        12 * 12,
        "each 2x2 district has one site, across zero"
    );
}

#[test]
fn natural_stone_age_material_is_physically_gatherable_finite_and_persistent() {
    let registries = build_registries();
    let seed = WorldGenerationSeed::new(7);
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
    let spawn = VoxelCoord::new(0, terrain_height(seed, 0, 0) + 1, 0);
    validate_admit_player(&registries, &state, spawn)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    validate_generate_player_neighborhood(&registries, &state)
        .unwrap()
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let resource = state
        .surface()
        .resources()
        .find(|record| record.commodity().form() != FORM_FOOD)
        .expect("seed 7 should expose a naturally generated primitive material");
    let source = resource.id();
    let commodity = resource.commodity();
    let position = resource.position();
    let before_mass = resource.remaining_mass();
    let total = calculate_matter_accounting(&state).unwrap().total();
    assert!(
        state.logistics().fluid_stores_at(position).next().is_none(),
        "generated raw material cannot overlap a finite spring"
    );
    crate::logistics::relocate_player_for_test(&mut state, position);
    let option = player_surface_gathering_options(&registries, &state)
        .unwrap()
        .into_iter()
        .find(|option| option.resource() == source)
        .expect("a natural material source must be obtainable with ordinary player actions");
    let sample = Mass::from_milligrams(25_000);
    validate_start_player_surface_gathering_option(&registries, &state, option, sample)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    for _ in 0..350 {
        if state.player_work().active().is_none() {
            break;
        }
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert!(
        state.player_work().active().is_none(),
        "raw material gathering must finish"
    );
    assert_eq!(
        state.surface().get(source).unwrap().remaining_mass(),
        before_mass.checked_sub(sample).unwrap(),
    );
    let carried = state.logistics().player().unwrap().carried_stockpile();
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .unwrap()
            .get_mass(commodity),
        sample
    );
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), total);
    if let Some(expansion) = validate_generate_player_neighborhood(&registries, &state).unwrap() {
        // Walking to a nearby source may reveal another chunk. This can add
        // new finite material, but never refill the already worked source.
        expansion.commit(&mut state).unwrap();
    }
    assert_eq!(
        state.surface().get(source).unwrap().remaining_mass(),
        before_mass.checked_sub(sample).unwrap(),
        "new chunk generation cannot refill the original material parcel",
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let restored = serde_json::from_slice::<LoadedSaveEnvelope>(&bytes)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(restored, state);
}
