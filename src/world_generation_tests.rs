//! Contracts for deterministic fresh-world wilderness surface generation.

use crate::content::{
    FORM_FOOD, FORM_LOG, FORM_LUMP, MATERIAL_BERRIES, MATERIAL_STONE, MATERIAL_WOOD,
    build_registries,
};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::{AppState, apply_clock_advance};
use crate::core::time::SimulationTick;
use crate::material::{CommodityKey, MaterialComposition};
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::spatial::VoxelCoord;
use crate::surface::{GeneratedSurfaceResourceSpec, validate_generate_surface_resources};

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
    validate_generate_initial_wilderness_surface(&registries, state, seed, spawn)
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
    assert_eq!(generated_profile(&first), generated_profile(&replay));

    let profile = generated_profile(&first);
    assert_eq!(profile.len(), 3);
    assert!(profile.iter().all(|(_, _, position)| *position == spawn));
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
fn initial_wilderness_generation_requires_fresh_pre_admission_surface_state() {
    let registries = build_registries();
    let spawn = VoxelCoord::new(0, 0, 0);
    let seed = WorldGenerationSeed::new(7);

    let mut generated = AppState::new();
    generate(&mut generated, seed, spawn);
    let before_second = generated.clone();
    assert_eq!(
        validate_generate_initial_wilderness_surface(&registries, &generated, seed, spawn).err(),
        Some(InitialWildernessGenerationError::SurfaceAlreadyGenerated)
    );
    assert_eq!(generated, before_second);

    let mut admitted = AppState::new();
    validate_admit_player(&registries, &admitted, spawn)
        .unwrap_or_else(|error| panic!("world-generation admission setup failed: {error}"))
        .commit(&mut admitted)
        .unwrap_or_else(|error| panic!("world-generation admission commit failed: {error}"));
    let before_admitted = admitted.clone();
    assert_eq!(
        validate_generate_initial_wilderness_surface(&registries, &admitted, seed, spawn).err(),
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
        validate_generate_initial_wilderness_surface(&registries, &aged, seed, spawn).err(),
        Some(InitialWildernessGenerationError::NotInitialTick {
            tick: SimulationTick::new(1),
        })
    );
    assert_eq!(aged, before_aged);
}

#[test]
fn initial_wilderness_generation_rejects_stale_surface_commit_without_mutation() {
    let registries = build_registries();
    let spawn = VoxelCoord::new(0, 0, 0);
    let validated = validate_generate_initial_wilderness_surface(
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
    let validated =
        validate_generate_initial_wilderness_surface(&registries, &admitted, seed, spawn)
            .unwrap_or_else(|error| {
                panic!("stale admission generation validation failed: {error}")
            });
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
    let validated = validate_generate_initial_wilderness_surface(&registries, &aged, seed, spawn)
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
fn generated_wilderness_surface_persists_and_remains_actor_local_after_load() {
    let registries = build_registries();
    let spawn = VoxelCoord::new(-3, 1, 5);
    let seed = WorldGenerationSeed::new(0xCAFE_BABE);
    let mut state = AppState::new();
    validate_generate_initial_wilderness_surface(&registries, &state, seed, spawn)
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
}
