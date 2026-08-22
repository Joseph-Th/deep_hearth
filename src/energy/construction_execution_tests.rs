//! Tests for the sibling construction execution module; isolated so test-only edits do not invalidate production builds.

use super::*;
use crate::content::{
    ENERGY_STONE_FLYWHEEL_DRIVE, FORM_FLYWHEEL, FORM_HANDLE, MATERIAL_STONE, MATERIAL_WOOD,
    build_registries,
};
use crate::core::quantity::Temperature;
use crate::core::state::{StateValidationError, validate_loaded_state};
use crate::core::time::WorldSeed;
use crate::energy::{AddEnergyStoreError, EnergyValidationError, add_energy_store};
use crate::inventory::{add_solid_stockpile_for_test, deposit_lot_for_test};
use crate::material::CommodityKey;
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
use crate::simulation::advance_tick;

fn assembled_store_fixture() -> (Registries, AppState, EnergyStoreId) {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xE57E_0001));
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_100_000))
        .unwrap_or_else(|error| panic!("energy assembly stockpile fixture failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
        Mass::from_milligrams(900_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("energy assembly flywheel fixture failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
        Mass::from_milligrams(200_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("energy assembly shaft fixture failed: {error}"));

    let store =
        validate_assemble_energy_store(&registries, &state, ENERGY_STONE_FLYWHEEL_DRIVE, source)
            .unwrap_or_else(|error| {
                panic!("energy-store assembly fixture validation failed: {error}")
            })
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("energy-store assembly fixture commit failed: {error}"));
    (registries, state, store)
}

#[test]
fn buildable_energy_store_requires_material_and_preserves_world_matter() {
    let registries = build_registries();
    let mut empty_state = AppState::new(WorldSeed::new(0xE57E_0000));
    assert_eq!(
        add_energy_store(&registries, &mut empty_state, ENERGY_STONE_FLYWHEEL_DRIVE,),
        Err(AddEnergyStoreError::RequiresAssembly {
            definition: ENERGY_STONE_FLYWHEEL_DRIVE,
        })
    );

    let (registries, state, store) = assembled_store_fixture();
    let record = state
        .energy()
        .get_store(store)
        .unwrap_or_else(|| panic!("assembled energy store disappeared"));
    assert_eq!(record.stored(), Energy::ZERO);
    assert_eq!(record.embodied_mass(), Mass::from_milligrams(1_100_000));
    assert_eq!(record.embodied_material().len(), 2);
    let accounting = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("assembled-store matter accounting failed: {error}"));
    assert_eq!(
        accounting.energy_storage(),
        crate::core::quantity::AggregateMass::from_milligrams(1_100_000)
    );
    assert_eq!(accounting.total(), accounting.energy_storage());
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn load_rejects_forged_energy_store_embodied_mass() {
    let (registries, state, store) = assembled_store_fixture();
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("energy embodiment tamper serialization failed: {error}"));
    encoded["state"]["systems"]["energy"]["records"][store.value().to_string()]["embodied_mass"] =
        serde_json::json!(1_000_000_u64);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("energy embodiment tamper decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Energy(
            EnergyValidationError::EmbodiedTraceMassMismatch {
                store,
                stored: Mass::from_milligrams(1_000_000),
                traced: Mass::from_milligrams(1_100_000),
            }
        )))
    );
}

#[test]
fn load_rejects_energy_store_material_created_after_construction() {
    let (registries, mut state, store) = assembled_store_fixture();
    advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("energy provenance audit tick failed: {error}"));
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("energy provenance tamper serialization failed: {error}"));
    let trace = &mut encoded["state"]["systems"]["energy"]["records"][store.value().to_string()]["embodied_material"]
        [0]["provenance"];
    trace["earliest_created_at"] = serde_json::json!(1_u64);
    trace["latest_created_at"] = serde_json::json!(1_u64);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("energy provenance tamper decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Energy(
            EnergyValidationError::EmbodiedProvenanceAfterConstruction {
                store,
                latest_created_at: crate::core::time::SimulationTick::new(1),
                created_at: crate::core::time::SimulationTick::ZERO,
            }
        )))
    );
}
