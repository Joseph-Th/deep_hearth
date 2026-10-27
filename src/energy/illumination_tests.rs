//! Contracts for finite renderer-neutral local illumination.

use crate::content::{
    BLOCK_STATE_STONE, CAMPFIRE_CHIP_FUEL_MASS, CAMPFIRE_FUEL_MASS, ENERGY_STONE_RING_CAMPFIRE,
    FORM_CHIP, FORM_LOG, FORM_LUMP, MATERIAL_STONE, MATERIAL_WOOD, build_registries,
    make_test_registries_with_energy_store,
};
use crate::core::quantity::{Energy, Mass, Power};
use crate::core::state::AppState;
use crate::energy::{
    EnergyCarrier, EnergyStoreDefinition, EnergyStoreDefinitionId,
    add_energy_store_with_initial_for_fixture, passive_dissipation_ticks_until_empty,
    validate_assemble_energy_store, validate_combust_fuel, validate_combust_selected_fuel,
};
use crate::inventory::deposit_bulk_for_test;
use crate::logistics::{place_energy_store_for_fixture, relocate_player_for_test};
use crate::material::CommodityKey;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::simulation::advance_tick;
use crate::spatial::VoxelCoord;
use crate::world::{
    BlockStateId, ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y, WorldGenerationSeed,
    set_generated_block_for_test, validate_generate_chunk,
};

use super::{assess_local_illumination, project_local_light_sources};

#[test]
fn ungenerated_opening_light_reaches_nearby_cells_and_replays() {
    let registries = build_registries();
    let mut state = AppState::new();
    let source = VoxelCoord::new(0, 0, 0);
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
    assert!(state.world().generation_profile().is_none());
    let at = |state: &AppState, x| {
        assess_local_illumination(&registries, state, VoxelCoord::new(x, 0, 0))
    };
    assert_eq!(at(&state, 0).level(), 7);
    assert_eq!(at(&state, 1).level(), 6);
    assert_eq!(at(&state, 5).level(), 2);
    assert_eq!(at(&state, 7).level(), 0);
    assert_eq!(at(&state, 1).strongest_source(), Some(fire));

    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    for x in 0..=7 {
        assert_eq!(at(&state, x), at(&loaded, x));
    }
}

#[test]
fn generated_solid_cell_shields_a_located_emitter_until_its_source_is_exposed() {
    let registries = build_registries();
    let mut state = AppState::new();
    let source = VoxelCoord::new(8, -10, 8);
    let adjacent = VoxelCoord::new(9, -10, 8);
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
    let stored_charge = state.energy().get_store(fire).unwrap().stored();
    // Preserve the real assembled, located fire while moving the player beyond
    // the incoming chunk's footprint before admitting generated terrain.
    relocate_player_for_test(&mut state, VoxelCoord::new(24, -10, 8));
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(0x4C49_4748_545F_534F),
        ChunkCoord::containing(source),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    assert_eq!(state.world().block_at(source), Some(BLOCK_STATE_STONE));
    assert_eq!(
        state.energy().get_store(fire).unwrap().stored(),
        stored_charge
    );
    set_generated_block_for_test(registries.world(), &mut state, adjacent, BlockStateId::AIR);
    assert_eq!(
        assess_local_illumination(&registries, &state, adjacent).level(),
        0,
        "light must not cross from an emitter enclosed within an opaque world cell"
    );
    assert!(
        project_local_light_sources(&registries, &state).is_empty(),
        "renderer projection must not advertise a buried light source"
    );
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(
        assess_local_illumination(&registries, &loaded, adjacent).level(),
        0
    );
    set_generated_block_for_test(registries.world(), &mut state, source, BlockStateId::AIR);
    assert_eq!(project_local_light_sources(&registries, &state).len(), 1);
    assert_eq!(
        state.energy().get_store(fire).unwrap().stored(),
        stored_charge
    );
    assert_eq!(
        assess_local_illumination(&registries, &state, adjacent).level(),
        6,
        "opening the actual source cell restores its ordinary finite light"
    );
}

#[test]
fn finite_campfire_light_appears_only_while_stored_heat_exists() {
    let registries = build_registries();
    let mut state = AppState::new();
    let horizontal = VoxelCoord::new(8, 0, 8);
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(0x4C49_4748_545F_5445),
        ChunkCoord::containing(horizontal),
        BLOCK_STATE_STONE,
    )
    .unwrap_or_else(|error| panic!("light fixture terrain generation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("light fixture terrain commit failed: {error}"));
    let surface_y = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
        .rev()
        .find(|&y| {
            state
                .world()
                .block_at(VoxelCoord::new(horizontal.x(), y, horizontal.z()))
                == Some(BLOCK_STATE_STONE)
        })
        .unwrap_or_else(|| panic!("light fixture generated column has no surface"));
    let origin = VoxelCoord::new(horizontal.x(), surface_y + 1, horizontal.z());
    for x in origin.x()..=origin.x() + 7 {
        set_generated_block_for_test(
            registries.world(),
            &mut state,
            VoxelCoord::new(x, origin.y() - 1, origin.z()),
            BLOCK_STATE_STONE,
        );
        for y in origin.y()..=origin.y() + 1 {
            set_generated_block_for_test(
                registries.world(),
                &mut state,
                VoxelCoord::new(x, y, origin.z()),
                BlockStateId::AIR,
            );
        }
    }
    validate_admit_player(&registries, &state, origin)
        .unwrap_or_else(|error| panic!("light fixture player admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("light fixture player admission commit failed: {error}"));
    let carried = state
        .logistics()
        .player()
        .map(|player| player.carried_stockpile())
        .unwrap_or_else(|| panic!("light fixture player disappeared"));
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
        deposit_bulk_for_test(&registries, &mut state, carried, commodity, mass)
            .unwrap_or_else(|error| panic!("light fixture material setup failed: {error}"));
    }
    let fire =
        validate_assemble_energy_store(&registries, &state, ENERGY_STONE_RING_CAMPFIRE, carried)
            .unwrap_or_else(|error| panic!("light fixture campfire assembly failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("light fixture campfire commit failed: {error}"));

    assert!(project_local_light_sources(&registries, &state).is_empty());
    assert_eq!(
        assess_local_illumination(&registries, &state, origin).level(),
        0
    );
    validate_combust_fuel(&registries, &state, fire, carried)
        .unwrap_or_else(|error| panic!("light fixture combustion failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("light fixture combustion commit failed: {error}"));

    let at_fire = assess_local_illumination(&registries, &state, origin);
    let sources = project_local_light_sources(&registries, &state);
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].store(), fire);
    assert_eq!(sources[0].position(), origin);
    assert_eq!(sources[0].level(), 7);
    assert_eq!(at_fire.level(), 7);
    assert_eq!(at_fire.strongest_source(), Some(fire));
    let one_away = VoxelCoord::new(origin.x() + 1, origin.y(), origin.z());
    let two_away = VoxelCoord::new(origin.x() + 2, origin.y(), origin.z());
    let six_away = VoxelCoord::new(origin.x() + 6, origin.y(), origin.z());
    let seven_away = VoxelCoord::new(origin.x() + 7, origin.y(), origin.z());
    assert_eq!(
        assess_local_illumination(&registries, &state, one_away).level(),
        6
    );
    assert_eq!(
        assess_local_illumination(&registries, &state, six_away).level(),
        1
    );
    assert_eq!(
        assess_local_illumination(&registries, &state, seven_away).level(),
        0
    );

    set_generated_block_for_test(registries.world(), &mut state, one_away, BLOCK_STATE_STONE);
    assert_eq!(
        assess_local_illumination(&registries, &state, one_away).level(),
        0,
        "an opaque world cell must not carry local block light"
    );
    assert_eq!(
        assess_local_illumination(&registries, &state, two_away).level(),
        3,
        "local block light should route around one opaque cell with the longer path cost"
    );
    set_generated_block_for_test(registries.world(), &mut state, one_away, BlockStateId::AIR);

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("lit campfire save failed: {error}"));
    let loaded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("lit campfire decode failed: {error}"));
    let loaded = loaded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("lit campfire replay failed: {error}"));
    assert_eq!(
        assess_local_illumination(&registries, &loaded, origin),
        at_fire,
        "local illumination must derive identically from trusted loaded energy/location state"
    );
    assert_eq!(project_local_light_sources(&registries, &loaded), sources);

    let stored = state
        .energy()
        .get_store(fire)
        .map(|record| record.stored())
        .unwrap_or_else(|| panic!("lit campfire disappeared before burn-down"));
    let horizon =
        passive_dissipation_ticks_until_empty(&registries, ENERGY_STONE_RING_CAMPFIRE, stored)
            .unwrap_or_else(|error| panic!("campfire light horizon projection failed: {error}"))
            .unwrap_or_else(|| {
                panic!("campfire light source must have finite passive dissipation")
            });
    let halfway = horizon.value() / 2;
    for tick in 1..=horizon.value() {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("campfire light burn-down tick failed: {error}"));
        if tick == halfway {
            let dimmed = assess_local_illumination(&registries, &state, origin);
            assert!(dimmed.level() > 0 && dimmed.level() < at_fire.level());
            let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
                .unwrap_or_else(|error| panic!("dim fire save failed: {error}"));
            let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
                .unwrap_or_else(|error| panic!("dim fire decode failed: {error}"));
            let loaded = decoded
                .into_state(&registries)
                .unwrap_or_else(|error| panic!("dim fire load failed: {error}"));
            assert_eq!(
                assess_local_illumination(&registries, &loaded, origin),
                dimmed
            );
        }
    }
    assert!(
        state
            .energy()
            .get_store(fire)
            .is_some_and(|record| record.stored().is_zero())
    );
    assert!(project_local_light_sources(&registries, &state).is_empty());
    assert_eq!(
        assess_local_illumination(&registries, &state, origin).level(),
        0
    );

    // Recoverable fabrication chips produce a shorter, dimmer but still useful light
    // source, rather than the same full-brightness flame as an intact log charge.
    deposit_bulk_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_WOOD, FORM_CHIP),
        CAMPFIRE_CHIP_FUEL_MASS,
    )
    .unwrap_or_else(|error| panic!("chip-light fuel material failed: {error}"));
    validate_combust_selected_fuel(
        &registries,
        &state,
        fire,
        carried,
        CommodityKey::new(MATERIAL_WOOD, FORM_CHIP),
    )
    .unwrap_or_else(|error| panic!("chip-light fuel validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("chip-light fuel commit failed: {error}"));
    let chip_light = assess_local_illumination(&registries, &state, origin);
    assert_eq!(chip_light.strongest_source(), Some(fire));
    assert!(chip_light.level() > 0 && chip_light.level() < at_fire.level());
    assert_eq!(
        project_local_light_sources(&registries, &state)[0].level(),
        chip_light.level()
    );
}

#[test]
fn local_light_index_excludes_remote_emitters_rebuilds_and_preserves_stable_ties() {
    let definition = EnergyStoreDefinitionId::new(930_405);
    let registries = make_test_registries_with_energy_store(
        EnergyStoreDefinition::new_with_transfer_limits(
            definition,
            "bounded resident light",
            EnergyCarrier::Electrical,
            Energy::from_nanojoules(1_000),
            Power::ZERO,
            Power::from_microwatts(10),
        )
        .with_passive_dissipation_power(Power::from_microwatts(1))
        .with_local_light_source(15),
    );
    let mut state = AppState::new();
    let center = VoxelCoord::new(0, 0, 0);
    let placements = [
        center,
        VoxelCoord::new(14, 0, 0),
        VoxelCoord::new(15, 0, 0),
        center,
        VoxelCoord::new(-15, 0, 0),
        VoxelCoord::new(900_000, 0, 0),
        VoxelCoord::new(i64::MAX, 0, 0),
    ];
    let mut ids = Vec::new();
    for position in placements {
        let store = add_energy_store_with_initial_for_fixture(
            &registries,
            &mut state,
            definition,
            Energy::from_nanojoules(1_000),
        )
        .unwrap();
        place_energy_store_for_fixture(&mut state, store, position);
        ids.push(store);
    }
    let nearby = state
        .logistics()
        .energy_store_locations_near(center, 14)
        .into_iter()
        .map(|(store, _)| store)
        .collect::<Vec<_>>();
    assert_eq!(nearby, [ids[0], ids[1], ids[3]]);
    assert!(state.logistics().has_valid_energy_position_index());
    assert_eq!(
        project_local_light_sources(&registries, &state).len(),
        placements.len()
    );
    let before = assess_local_illumination(&registries, &state, center);
    assert_eq!(before.level(), 15);
    assert_eq!(before.strongest_source(), Some(ids[0]));

    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    assert!(
        !String::from_utf8_lossy(&saved).contains("energy_stores_by_position"),
        "location index must not become independent persistent world truth"
    );
    let envelope: LoadedSaveEnvelope = serde_json::from_slice(&saved).unwrap();
    let mut loaded = envelope.into_state(&registries).unwrap();
    assert_eq!(state, loaded);
    assert_eq!(
        loaded
            .logistics()
            .energy_store_locations_near(center, 14)
            .len(),
        3
    );
    assert_eq!(
        assess_local_illumination(&registries, &loaded, center),
        before
    );

    let old_revision = loaded.logistics().revision();
    loaded
        .logistics_state_mut()
        .apply_energy_store_location_removal(old_revision, old_revision + 1, ids[0], center);
    assert!(loaded.logistics().has_valid_energy_position_index());
    assert_eq!(
        assess_local_illumination(&registries, &loaded, center).strongest_source(),
        Some(ids[3]),
        "removing the oldest co-located emitter must preserve the next stable winner"
    );
    let revision = loaded.logistics().revision();
    loaded
        .logistics_state_mut()
        .apply_energy_store_location_removal(revision, revision + 1, ids[3], center);
    assert!(loaded.logistics().has_valid_energy_position_index());
    assert_eq!(
        loaded.logistics().energy_store_locations_near(center, 0),
        Vec::new(),
        "removing the last colocated source removes its empty position bucket"
    );
    let residual = assess_local_illumination(&registries, &loaded, center);
    assert_eq!(residual.level(), 1);
    assert_eq!(
        residual.strongest_source(),
        Some(ids[1]),
        "after colocated light is removed the real 14-voxel source still contributes one level"
    );
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &loaded),
        Ok(())
    );
}
