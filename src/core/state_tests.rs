//! Contract tests for root runtime state and trusted continuation.

use super::*;
use crate::content::build_registries;
use crate::content::{ENERGY_STONE_FLYWHEEL_DRIVE, FLUID_WATER};
use crate::core::quantity::{Mass, Temperature, Volume};
use crate::energy::validate_assemble_energy_store;
use crate::fluid::add_fluid_store_with_contents_for_fixture;
use crate::inventory::{add_solid_stockpile_for_test, deposit_lot_for_test};
use crate::logistics::{
    validate_initialize_player_logistics, validate_place_fluid_store,
    validate_place_ground_stockpile,
};
use crate::spatial::VoxelCoord;
#[test]
fn new_state_starts_at_zero_and_validates() {
    let registries = build_registries();
    let state = AppState::new();

    assert_eq!(state.tick(), SimulationTick::ZERO);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn app_state_debug_does_not_expose_hidden_geology() {
    let state = AppState::new();

    let debug = format!("{state:?}");

    assert!(debug.contains("geological_knowledge"));
    assert!(!debug.contains("geology:"));
}

#[test]
fn local_energy_observation_requires_actor_admission_and_exact_colocation() {
    let registries = build_registries();
    let mut state = AppState::new();
    let assembly_inputs = registries
        .energy()
        .get_store(ENERGY_STONE_FLYWHEEL_DRIVE)
        .and_then(|definition| definition.assembly_profile())
        .map(|profile| {
            profile
                .inputs()
                .iter()
                .map(|input| (input.commodity(), input.mass()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| panic!("local energy observation fixture lost authored assembly"));
    let assembly_mass = assembly_inputs
        .iter()
        .map(|(_, mass)| *mass)
        .try_fold(Mass::ZERO, Mass::checked_add)
        .unwrap_or_else(|| panic!("local energy observation assembly mass overflowed"));
    let mut assemble = |context: &'static str| {
        let source = add_solid_stockpile_for_test(&mut state, assembly_mass)
            .unwrap_or_else(|error| panic!("{context} energy source fixture failed: {error}"));
        for &(commodity, mass) in &assembly_inputs {
            deposit_lot_for_test(
                &registries,
                &mut state,
                source,
                commodity,
                mass,
                Temperature::from_millikelvin(293_150),
            )
            .unwrap_or_else(|error| panic!("{context} energy material fixture failed: {error}"));
        }
        validate_assemble_energy_store(&registries, &state, ENERGY_STONE_FLYWHEEL_DRIVE, source)
            .unwrap_or_else(|error| panic!("{context} energy assembly failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("{context} energy assembly commit failed: {error}"))
    };
    let local = assemble("local");
    let remote = assemble("remote");
    let _unlocated = assemble("unlocated");
    let player_position = VoxelCoord::new(0, 0, 0);
    state
        .logistics_state_mut()
        .apply_energy_store_placement(0, 1, local, player_position);
    state.logistics_state_mut().apply_energy_store_placement(
        1,
        2,
        remote,
        VoxelCoord::new(1, 0, 0),
    );

    assert!(
        state.available_local_energy_stores().next().is_none(),
        "energy stores must not be actor-observable before logistics admission"
    );

    validate_initialize_player_logistics(&state, player_position, Mass::from_milligrams(1))
        .unwrap_or_else(|error| panic!("local energy observation player admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("local energy observation player commit failed: {error}"));
    assert_eq!(
        state
            .available_local_energy_stores()
            .map(|store| store.id())
            .collect::<Vec<_>>(),
        vec![local],
        "local energy observation must exclude remote and unlocated stores"
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn local_fluid_observation_requires_actor_admission_and_exact_colocation() {
    let registries = build_registries();
    let mut state = AppState::new();
    let local = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        Volume::from_microliters(1_000_000),
        FLUID_WATER,
        Volume::from_microliters(750_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("local fluid observation fixture failed: {error}"));
    let remote = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        Volume::from_microliters(1_000_000),
        FLUID_WATER,
        Volume::from_microliters(750_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("remote fluid observation fixture failed: {error}"));
    let _unlocated = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        Volume::from_microliters(1_000_000),
        FLUID_WATER,
        Volume::from_microliters(750_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("unlocated fluid observation fixture failed: {error}"));
    let player_position = VoxelCoord::new(0, 0, 0);
    validate_place_fluid_store(&state, local, player_position)
        .unwrap_or_else(|error| panic!("local fluid placement failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("local fluid placement commit failed: {error}"));
    validate_place_fluid_store(&state, remote, VoxelCoord::new(1, 0, 0))
        .unwrap_or_else(|error| panic!("remote fluid placement failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("remote fluid placement commit failed: {error}"));

    assert!(
        state.available_local_fluid_stores().next().is_none(),
        "fluid stores must not be actor-observable before logistics admission"
    );

    validate_initialize_player_logistics(&state, player_position, Mass::from_milligrams(1))
        .unwrap_or_else(|error| panic!("local fluid observation player admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("local fluid observation player commit failed: {error}"));
    assert_eq!(
        state
            .available_local_fluid_stores()
            .map(|store| store.id())
            .collect::<Vec<_>>(),
        vec![local],
        "local fluid observation must exclude remote and unlocated stores"
    );
}

#[test]
fn local_ground_stockpile_observation_requires_actor_admission_and_exact_colocation() {
    let registries = build_registries();
    let mut state = AppState::new();
    let local = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(10))
        .unwrap_or_else(|error| panic!("local ground observation fixture failed: {error}"));
    let remote = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(10))
        .unwrap_or_else(|error| panic!("remote ground observation fixture failed: {error}"));
    let _unlocated = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(10))
        .unwrap_or_else(|error| panic!("unlocated ground observation fixture failed: {error}"));
    let player_position = VoxelCoord::new(0, 0, 0);
    validate_place_ground_stockpile(&state, local, player_position)
        .unwrap_or_else(|error| panic!("local ground placement failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("local ground placement commit failed: {error}"));
    validate_place_ground_stockpile(&state, remote, VoxelCoord::new(1, 0, 0))
        .unwrap_or_else(|error| panic!("remote ground placement failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("remote ground placement commit failed: {error}"));

    assert!(
        state.available_local_ground_stockpiles().next().is_none(),
        "ground stockpiles must not be actor-observable before logistics admission"
    );

    let carried =
        validate_initialize_player_logistics(&state, player_position, Mass::from_milligrams(10))
            .unwrap_or_else(|error| {
                panic!("local ground observation player admission failed: {error}")
            })
            .commit(&mut state)
            .unwrap_or_else(|error| {
                panic!("local ground observation player commit failed: {error}")
            })
            .carried_stockpile();
    assert_eq!(
        state
            .available_local_ground_stockpiles()
            .map(|stockpile| stockpile.id())
            .collect::<Vec<_>>(),
        vec![local],
        "local ground observation must exclude carried, remote, and unlocated stockpiles"
    );
    assert_ne!(local, carried);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[cfg(feature = "test-soak")]
#[path = "state_tests/soak.rs"]
mod soak;
