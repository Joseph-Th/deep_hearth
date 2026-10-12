//! Contracts for deterministic fresh-world wilderness opening generation.

use crate::content::{
    FLUID_WATER, FORM_FOOD, FORM_LOG, FORM_LUMP, MATERIAL_BERRIES, MATERIAL_STONE, MATERIAL_WOOD,
    build_registries,
};
use crate::core::quantity::{Mass, Temperature, Volume};
use crate::core::state::{AppState, apply_clock_advance};
use crate::core::time::SimulationTick;
use crate::fluid::add_fluid_store_with_contents_for_fixture;
use crate::logistics::validate_allocate_ground_stockpile;
use crate::material::{CommodityKey, MaterialComposition};
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::spatial::VoxelCoord;
use crate::surface::{GeneratedSurfaceResourceSpec, validate_generate_surface_resources};
use crate::survival::{available_adjacent_drink_sources, available_local_drink_sources};

use super::*;

fn generated_profile(state: &AppState) -> Vec<(CommodityKey, Mass, VoxelCoord)> {
    state
        .surface()
        .resources()
        .map(|record| {
            (
                record.commodity(),
                record.remaining_mass(),
                record.position(),
            )
        })
        .collect()
}

fn generate(state: &mut AppState, seed: WorldGenerationSeed, spawn: VoxelCoord) {
    let registries = build_registries();
    validate_generate_initial_wilderness(&registries, state, seed, spawn)
        .unwrap_or_else(|error| panic!("wilderness generation validation failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("wilderness generation commit failed: {error}"));
}

#[test]
fn initial_wilderness_generation_is_deterministic_and_seed_sensitive() {
    let spawn = VoxelCoord::new(12, 3, -8);
    let seed = WorldGenerationSeed::new(0xD33F_0001_5EED_A11C);
    let mut first = AppState::new();
    let mut replay = AppState::new();
    generate(&mut first, seed, spawn);
    generate(&mut replay, seed, spawn);
    assert_eq!(
        first, replay,
        "identical explicit seeds must replay the whole generated opening"
    );
    assert_eq!(generated_profile(&first), generated_profile(&replay));

    let profile = generated_profile(&first);
    assert_eq!(profile.len(), 3);
    assert!(profile.iter().all(|(_, _, position)| *position == spawn));
    let water = first
        .fluid()
        .stores()
        .next()
        .unwrap_or_else(|| panic!("generated wilderness water disappeared"));
    assert_eq!(first.fluid().stores().count(), 1);
    let contents = water
        .contents()
        .unwrap_or_else(|| panic!("generated wilderness water store is empty"));
    assert_eq!(contents.fluid(), FLUID_WATER);
    assert_eq!(
        contents.volume(),
        Volume::from_microliters(OPENING_WATER_VOLUME_UL)
    );
    assert_eq!(
        water.capacity(),
        Volume::from_microliters(OPENING_WATER_VOLUME_UL)
    );
    assert_eq!(contents.temperature(), WILDERNESS_TEMPERATURE);
    let water_position = first
        .logistics()
        .fluid_store_position(water.id())
        .unwrap_or_else(|| panic!("generated wilderness water has no world location"));
    assert!(spawn.is_face_adjacent_to(water_position));
    assert_eq!(water_position.y(), spawn.y());
    let mass_for = |commodity| {
        profile
            .iter()
            .find(|(found, _, _)| *found == commodity)
            .map(|(_, mass, _)| *mass)
            .unwrap_or_else(|| panic!("generated wilderness commodity {commodity:?} disappeared"))
    };
    assert!(
        (Mass::from_milligrams(STONE_BASE_MASS_MG)
            ..=Mass::from_milligrams(STONE_BASE_MASS_MG + STONE_VARIATION_MG))
            .contains(&mass_for(CommodityKey::new(MATERIAL_STONE, FORM_LUMP)))
    );
    assert!(
        (Mass::from_milligrams(TIMBER_BASE_MASS_MG)
            ..=Mass::from_milligrams(TIMBER_BASE_MASS_MG + TIMBER_VARIATION_MG))
            .contains(&mass_for(CommodityKey::new(MATERIAL_WOOD, FORM_LOG)))
    );
    assert!(
        (Mass::from_milligrams(BERRIES_BASE_MASS_MG)
            ..=Mass::from_milligrams(BERRIES_BASE_MASS_MG + BERRIES_VARIATION_MG))
            .contains(&mass_for(CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD)))
    );

    let mut alternate = AppState::new();
    generate(
        &mut alternate,
        WorldGenerationSeed::new(seed.value() ^ 1),
        spawn,
    );
    assert_ne!(
        generated_profile(&first),
        generated_profile(&alternate),
        "different explicit world seeds should change at least one opening opportunity"
    );
}

#[test]
fn initial_wilderness_generation_requires_fresh_pre_admission_world_state() {
    let registries = build_registries();
    let spawn = VoxelCoord::new(0, 0, 0);
    let seed = WorldGenerationSeed::new(7);

    let mut generated = AppState::new();
    generate(&mut generated, seed, spawn);
    let before_second = generated.clone();
    assert_eq!(
        validate_generate_initial_wilderness(&registries, &generated, seed, spawn).err(),
        Some(InitialWildernessGenerationError::SurfaceAlreadyGenerated)
    );
    assert_eq!(generated, before_second);

    let mut fluid_generated = AppState::new();
    add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut fluid_generated,
        Volume::from_microliters(1),
        FLUID_WATER,
        Volume::from_microliters(1),
        WILDERNESS_TEMPERATURE,
    )
    .unwrap_or_else(|error| panic!("preexisting fluid generation fixture failed: {error}"));
    let before_fluid = fluid_generated.clone();
    assert_eq!(
        validate_generate_initial_wilderness(&registries, &fluid_generated, seed, spawn).err(),
        Some(InitialWildernessGenerationError::FluidAlreadyGenerated)
    );
    assert_eq!(fluid_generated, before_fluid);

    let mut admitted = AppState::new();
    validate_admit_player(&registries, &admitted, spawn)
        .unwrap_or_else(|error| panic!("world-generation admission setup failed: {error}"))
        .commit(&mut admitted)
        .unwrap_or_else(|error| panic!("world-generation admission commit failed: {error}"));
    let before_admitted = admitted.clone();
    assert_eq!(
        validate_generate_initial_wilderness(&registries, &admitted, seed, spawn).err(),
        Some(InitialWildernessGenerationError::PlayerAlreadyInitialized {
            survival_initialized: true,
            logistics_initialized: true,
        })
    );
    assert_eq!(admitted, before_admitted);

    let mut aged = AppState::new();
    apply_clock_advance(&mut aged, SimulationTick::new(1));
    let before_aged = aged.clone();
    assert_eq!(
        validate_generate_initial_wilderness(&registries, &aged, seed, spawn).err(),
        Some(InitialWildernessGenerationError::NotInitialTick {
            tick: SimulationTick::new(1),
        })
    );
    assert_eq!(aged, before_aged);
}

#[test]
fn initial_wilderness_generation_rejects_stale_fluid_commit_without_surface_mutation() {
    let registries = build_registries();
    let spawn = VoxelCoord::new(0, 0, 0);
    let validated = validate_generate_initial_wilderness(
        &registries,
        &AppState::new(),
        WorldGenerationSeed::new(12),
        spawn,
    )
    .unwrap_or_else(|error| panic!("stale fluid generation validation failed: {error}"));

    let mut state = AppState::new();
    add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        Volume::from_microliters(1),
        FLUID_WATER,
        Volume::from_microliters(1),
        WILDERNESS_TEMPERATURE,
    )
    .unwrap_or_else(|error| panic!("competing fluid generation failed: {error}"));
    let before = state.clone();
    assert_eq!(
        validated.commit(&mut state),
        Err(InitialWildernessGenerationCommitError::StaleFluidRevision {
            expected: 0,
            actual: 1,
        })
    );
    assert_eq!(state, before);
    assert!(state.surface().resources().next().is_none());
}

#[test]
fn initial_wilderness_generation_rejects_stale_logistics_commit_without_owner_mutation() {
    let registries = build_registries();
    let spawn = VoxelCoord::new(0, 0, 0);
    let validated = validate_generate_initial_wilderness(
        &registries,
        &AppState::new(),
        WorldGenerationSeed::new(14),
        spawn,
    )
    .unwrap_or_else(|error| panic!("stale logistics generation validation failed: {error}"));

    let mut state = AppState::new();
    validate_allocate_ground_stockpile(&state, VoxelCoord::new(8, 0, 0), Mass::from_milligrams(1))
        .unwrap_or_else(|error| panic!("competing logistics allocation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("competing logistics allocation commit failed: {error}"));
    let before = state.clone();
    assert_eq!(
        validated.commit(&mut state),
        Err(
            InitialWildernessGenerationCommitError::StaleLogisticsRevision {
                expected: 0,
                actual: 1,
            }
        )
    );
    assert_eq!(state, before);
    assert!(state.surface().resources().next().is_none());
    assert!(state.fluid().stores().next().is_none());
}

#[test]
fn initial_wilderness_generation_rejects_stale_surface_commit_without_mutation() {
    let registries = build_registries();
    let spawn = VoxelCoord::new(0, 0, 0);
    let validated = validate_generate_initial_wilderness(
        &registries,
        &AppState::new(),
        WorldGenerationSeed::new(11),
        spawn,
    )
    .unwrap_or_else(|error| panic!("stale world generation validation failed: {error}"));

    let mut state = AppState::new();
    let competing = GeneratedSurfaceResourceSpec::new(
        spawn,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(1),
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_STONE),
    )
    .unwrap_or_else(|error| panic!("competing surface spec failed: {error}"));
    validate_generate_surface_resources(&registries, &state, [competing])
        .unwrap_or_else(|error| panic!("competing surface generation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("competing surface generation commit failed: {error}"));
    let before = state.clone();

    assert_eq!(
        validated.commit(&mut state),
        Err(
            InitialWildernessGenerationCommitError::StaleSurfaceRevision {
                expected: 0,
                actual: 1,
            }
        )
    );
    assert_eq!(state, before);
}

#[test]
fn initial_wilderness_generation_rejects_admission_or_time_change_before_commit() {
    let registries = build_registries();
    let spawn = VoxelCoord::new(0, 0, 0);
    let seed = WorldGenerationSeed::new(13);

    let mut admitted = AppState::new();
    let validated = validate_generate_initial_wilderness(&registries, &admitted, seed, spawn)
        .unwrap_or_else(|error| panic!("stale admission generation validation failed: {error}"));
    validate_admit_player(&registries, &admitted, spawn)
        .unwrap_or_else(|error| panic!("stale admission player validation failed: {error}"))
        .commit(&mut admitted)
        .unwrap_or_else(|error| panic!("stale admission player commit failed: {error}"));
    let before_admitted_commit = admitted.clone();
    assert_eq!(
        validated.commit(&mut admitted),
        Err(InitialWildernessGenerationCommitError::PlayerInitialized {
            survival_initialized: true,
            logistics_initialized: true,
        })
    );
    assert_eq!(admitted, before_admitted_commit);

    let mut aged = AppState::new();
    let validated = validate_generate_initial_wilderness(&registries, &aged, seed, spawn)
        .unwrap_or_else(|error| panic!("stale tick generation validation failed: {error}"));
    apply_clock_advance(&mut aged, SimulationTick::new(1));
    let before_aged_commit = aged.clone();
    assert_eq!(
        validated.commit(&mut aged),
        Err(InitialWildernessGenerationCommitError::TickChanged {
            expected: SimulationTick::ZERO,
            actual: SimulationTick::new(1),
        })
    );
    assert_eq!(aged, before_aged_commit);
}

#[test]
fn generated_wilderness_persists_with_local_surface_and_adjacent_water_after_load() {
    let registries = build_registries();
    let spawn = VoxelCoord::new(-3, 1, 5);
    let seed = WorldGenerationSeed::new(0xCAFE_BABE);
    let mut state = AppState::new();
    validate_generate_initial_wilderness(&registries, &state, seed, spawn)
        .unwrap_or_else(|error| panic!("persistent wilderness generation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("persistent wilderness generation commit failed: {error}"));
    validate_admit_player(&registries, &state, spawn)
        .unwrap_or_else(|error| panic!("persistent wilderness admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("persistent wilderness admission commit failed: {error}"));
    let visible_before = state
        .available_surface_resources()
        .map(|record| (record.commodity(), record.remaining_mass()))
        .collect::<Vec<_>>();
    assert_eq!(
        available_local_drink_sources(&registries, &state).count(),
        0
    );
    let adjacent_before = available_adjacent_drink_sources(&registries, &state)
        .map(|source| (source.store(), source.fluid(), source.position()))
        .collect::<Vec<_>>();
    assert_eq!(adjacent_before.len(), 1);

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("wilderness generation save failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("wilderness generation decode failed: {error}"));
    let loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("wilderness generation load failed: {error}"));
    let visible_after = loaded
        .available_surface_resources()
        .map(|record| (record.commodity(), record.remaining_mass()))
        .collect::<Vec<_>>();
    assert_eq!(visible_after, visible_before);
    let adjacent_after = available_adjacent_drink_sources(&registries, &loaded)
        .map(|source| (source.store(), source.fluid(), source.position()))
        .collect::<Vec<_>>();
    assert_eq!(adjacent_after, adjacent_before);
    assert_eq!(
        available_local_drink_sources(&registries, &loaded).count(),
        0
    );
}
