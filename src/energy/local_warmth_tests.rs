//! Heat aggregation never overflows the finite thermal comfort budget.

use super::*;

#[test]
fn local_warmth_aggregation_caps_before_energy_overflow() {
    let maximum = Energy::from_nanojoules(u128::MAX - 10);
    let almost_maximum = Energy::from_nanojoules(u128::MAX - 20);

    let first = cap_warmth(Energy::ZERO, almost_maximum, maximum);
    assert_eq!(first, almost_maximum);
    assert_eq!(
        cap_warmth(first, Energy::from_nanojoules(100), maximum),
        maximum
    );
    assert_eq!(
        cap_warmth(maximum, Energy::from_nanojoules(u128::MAX), maximum),
        maximum
    );
}

#[test]
fn warmth_requires_authoritative_air_across_a_generated_boundary() {
    use crate::content::{
        BLOCK_STATE_STONE, CAMPFIRE_FUEL_MASS, ENERGY_STONE_RING_CAMPFIRE, FORM_LOG, FORM_LUMP,
        MATERIAL_STONE, MATERIAL_WOOD, build_registries,
    };
    use crate::core::quantity::Mass;
    use crate::core::state::validate_loaded_state;
    use crate::energy::{validate_assemble_energy_store, validate_combust_fuel};
    use crate::inventory::deposit_bulk_for_test;
    use crate::logistics::relocate_player_for_test;
    use crate::material::CommodityKey;
    use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
    use crate::player::validate_admit_player;
    use crate::world::{
        BlockStateId, ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y, WorldGenerationSeed,
        validate_generate_chunk,
    };

    let registries = build_registries();
    let mut state = AppState::new();
    let seed = WorldGenerationSeed::new(41);
    // Resolve a level pair of adjacent, supported cells from actual terrain
    // without committing either column. The heat source remains in graph-only
    // custody until its own chunk receives authoritative geometry.
    let left = validate_generate_chunk(
        &registries,
        &state,
        seed,
        ChunkCoord::new(-1, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap();
    let right = validate_generate_chunk(
        &registries,
        &state,
        seed,
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap();
    let (source, observer) = (0..16)
        .find_map(|z| {
            let floor_y = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
                .rev()
                .find(|&y| left.chunk().block_at(VoxelCoord::new(-1, y, z)) != BlockStateId::AIR)?;
            let source = VoxelCoord::new(-1, floor_y + 1, z);
            let observer = VoxelCoord::new(0, floor_y + 1, z);
            (right.chunk().block_at(VoxelCoord::new(0, floor_y, z)) == BLOCK_STATE_STONE
                && right.chunk().block_at(observer) == BlockStateId::AIR
                && right.chunk().block_at(VoxelCoord::new(0, floor_y + 2, z)) == BlockStateId::AIR)
                .then_some((source, observer))
        })
        .expect("seed has a level standing route across the chunk boundary");
    let carried = validate_admit_player(&registries, &state, source)
        .unwrap()
        .commit(&mut state)
        .unwrap()
        .carried_stockpile();
    for (commodity, mass) in [
        (
            CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
            Mass::from_milligrams(1_000_000),
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            CAMPFIRE_FUEL_MASS,
        ),
    ] {
        deposit_bulk_for_test(&registries, &mut state, carried, commodity, mass).unwrap();
    }
    let fire =
        validate_assemble_energy_store(&registries, &state, ENERGY_STONE_RING_CAMPFIRE, carried)
            .unwrap()
            .commit(&mut state)
            .unwrap();
    validate_combust_fuel(&registries, &state, fire, carried)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let charge = state.energy().get_store(fire).unwrap().stored();
    relocate_player_for_test(&mut state, observer);
    validate_generate_chunk(
        &registries,
        &state,
        seed,
        ChunkCoord::containing(observer),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let warmth = |state: &AppState| {
        let passive = crate::energy::passive_dissipation::decide_passive_energy_dissipation(
            &registries,
            state,
        );
        assess_player_local_warmth(
            &registries,
            state,
            &passive,
            Energy::from_nanojoules(u128::MAX),
        )
    };
    assert_eq!(
        crate::energy::assess_local_illumination(&registries, &state, observer).level(),
        0
    );
    assert!(project_player_nearby_warmth_sources(&registries, &state).is_empty());
    assert_eq!(warmth(&state), Energy::ZERO);
    assert_eq!(state.energy().get_store(fire).unwrap().stored(), charge);

    validate_generate_chunk(
        &registries,
        &state,
        seed,
        ChunkCoord::containing(source),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    assert_eq!(
        project_player_nearby_warmth_sources(&registries, &state).len(),
        1
    );
    assert!(!warmth(&state).is_zero());
    assert!(crate::energy::assess_local_illumination(&registries, &state, observer).level() > 0);
    assert_eq!(state.energy().get_store(fire).unwrap().stored(), charge);
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(replay, state);
    assert_eq!(warmth(&replay), warmth(&state));
}

#[test]
fn visible_nearby_fire_matches_physical_heat_without_granting_remote_access() {
    use crate::content::{
        CAMPFIRE_FUEL_MASS, ENERGY_STONE_RING_CAMPFIRE, FORM_LOG, FORM_LUMP, MATERIAL_STONE,
        MATERIAL_WOOD, build_registries,
    };
    use crate::energy::{validate_assemble_energy_store, validate_combust_fuel};
    use crate::inventory::deposit_bulk_for_test;
    use crate::logistics::relocate_player_for_test;
    use crate::material::CommodityKey;
    use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
    use crate::player::validate_admit_player;

    let registries = build_registries();
    let mut state = AppState::new();
    let at_fire = VoxelCoord::new(0, 0, 0);
    let beside_fire = VoxelCoord::new(1, 0, 0);
    let diagonal = VoxelCoord::new(1, 0, 1);
    let carried = validate_admit_player(&registries, &state, at_fire)
        .unwrap()
        .commit(&mut state)
        .unwrap()
        .carried_stockpile();
    for (commodity, mass) in [
        (
            CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
            crate::core::quantity::Mass::from_milligrams(1_000_000),
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            CAMPFIRE_FUEL_MASS,
        ),
    ] {
        deposit_bulk_for_test(&registries, &mut state, carried, commodity, mass).unwrap();
    }
    let fire =
        validate_assemble_energy_store(&registries, &state, ENERGY_STONE_RING_CAMPFIRE, carried)
            .unwrap()
            .commit(&mut state)
            .unwrap();
    assert!(project_player_nearby_warmth_sources(&registries, &state).is_empty());
    validate_combust_fuel(&registries, &state, fire, carried)
        .unwrap()
        .commit(&mut state)
        .unwrap();

    let projection = project_player_nearby_warmth_sources(&registries, &state);
    assert_eq!(projection.len(), 1);
    assert_eq!(projection[0].store(), fire);
    assert_eq!(projection[0].position(), at_fire);
    assert_eq!(projection[0].definition(), ENERGY_STONE_RING_CAMPFIRE);
    assert_eq!(
        projection[0].stored(),
        state.energy().get_store(fire).unwrap().stored()
    );

    let passive =
        crate::energy::passive_dissipation::decide_passive_energy_dissipation(&registries, &state);
    let full = assess_player_local_warmth(
        &registries,
        &state,
        &passive,
        Energy::from_nanojoules(u128::MAX),
    );
    assert!(!full.is_zero());
    relocate_player_for_test(&mut state, beside_fire);
    assert_eq!(
        project_player_nearby_warmth_sources(&registries, &state),
        projection,
        "face-adjacent warmth is observable even though its store is no longer local",
    );
    assert_eq!(state.available_local_energy_stores().count(), 0);
    assert_eq!(
        assess_player_local_warmth(
            &registries,
            &state,
            &passive,
            Energy::from_nanojoules(u128::MAX),
        )
        .nanojoules(),
        full.nanojoules() / 2,
    );

    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(
        project_player_nearby_warmth_sources(&registries, &loaded),
        projection,
        "heat observation must reproduce after trusted loading",
    );
    relocate_player_for_test(&mut state, diagonal);
    assert!(project_player_nearby_warmth_sources(&registries, &state).is_empty());
    assert_eq!(
        assess_player_local_warmth(
            &registries,
            &state,
            &passive,
            Energy::from_nanojoules(u128::MAX),
        ),
        Energy::ZERO,
    );
}
