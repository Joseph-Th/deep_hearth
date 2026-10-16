//! Contracts for finite renderer-neutral local illumination.

use crate::content::{
    BLOCK_STATE_STONE, CAMPFIRE_FUEL_MASS, ENERGY_STONE_RING_CAMPFIRE, FORM_LOG, FORM_LUMP,
    MATERIAL_STONE, MATERIAL_WOOD, build_registries,
};
use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::energy::{
    passive_dissipation_ticks_until_empty, validate_assemble_energy_store, validate_combust_fuel,
};
use crate::inventory::deposit_bulk_for_test;
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
    for _ in 0..horizon.value() {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("campfire light burn-down tick failed: {error}"));
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
}
