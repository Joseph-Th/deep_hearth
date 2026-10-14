//! Atomic ordinary-player admission and stale-owner rejection contracts.

use crate::content::{
    FORM_LOG, MATERIAL_WOOD, STANDARD_TEST_HEATER, STANDARD_TEST_HEATING_ENERGY, build_registries,
    make_test_registries_with_standard_sensible_heating,
};
use crate::core::quantity::{Energy, Mass, Temperature};
use crate::core::state::{AppState, StateValidationError};
use crate::energy::add_energy_store_with_initial_for_fixture;
use crate::equipment::add_equipment;
use crate::inventory::{
    MaterialLotSelection, StockpileStorageProfile, add_solid_stockpile_for_test, add_stockpile,
    deposit_lot_for_test,
};
use crate::logistics::{
    GroundStockpileAllocationError, LogisticsValidationError, validate_allocate_ground_stockpile,
    validate_player_logistics_initialization,
};
use crate::maintenance::Condition;
use crate::material::CommodityKey;
use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
use crate::production::{ProcessId, validate_start_process};
use crate::spatial::VoxelCoord;
use crate::survival::initialize_player_survival;
use crate::thermal::{SensibleHeatingRequest, resolve_sensible_heating_process};

use super::{PlayerAdmissionCommitError, PlayerAdmissionError, validate_admit_player};

#[test]
fn ordinary_player_admission_commits_all_owner_state_together() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(4, -2, 7);

    let admitted = validate_admit_player(&registries, &state, position)
        .unwrap_or_else(|error| panic!("player admission validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("player admission commit failed: {error}"));

    assert!(state.survival().player().is_some());
    assert_eq!(state.logistics().player().copied(), Some(admitted));
    assert_eq!(admitted.position(), position);
    let carried = state
        .inventory()
        .get_stockpile(admitted.carried_stockpile())
        .unwrap_or_else(|| panic!("admitted carried stockpile disappeared"));
    assert_eq!(
        carried.capacity(),
        registries.survival().physiology().carried_mass_capacity()
    );
    assert_eq!(carried.stored_mass(), Mass::ZERO);
}

#[test]
fn trusted_load_rejects_horizontal_body_velocity_above_authored_walk_speed() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 0))
        .unwrap_or_else(|error| panic!("velocity-bound admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("velocity-bound admission commit failed: {error}"));
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("velocity-bound serialization failed: {error}"));
    encoded["state"]["systems"]["logistics"]["player"]["body"]["horizontal_velocity"]["x_nanometers_per_second"] =
        serde_json::json!(4_300_000_001_i64);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("velocity-bound decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Logistics(
            LogisticsValidationError::PlayerHorizontalVelocityExceedsWalkingSpeed {
                x_nanometers_per_second: 4_300_000_001,
                z_nanometers_per_second: 0,
                maximum_nanometers_per_second: 4_300_000_000,
            },
        )))
    );
}

#[test]
fn ordinary_player_admission_rejects_body_extent_beyond_continuous_world() {
    let registries = build_registries();
    let state = AppState::new();
    let position = VoxelCoord::new(0, 9_223_372_036, 0);

    assert_eq!(
        validate_admit_player(&registries, &state, position).err(),
        Some(PlayerAdmissionError::PositionOutsideContinuousWorld { position })
    );
}

#[test]
fn trusted_load_rejects_player_body_extent_beyond_continuous_world() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 0))
        .unwrap_or_else(|error| panic!("body-bound load admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("body-bound load admission commit failed: {error}"));
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("body-bound load serialization failed: {error}"));
    encoded["state"]["systems"]["logistics"]["player"]["body"]["foot_position"]["y_nanometers"] =
        serde_json::json!(9_223_372_036_000_000_000_i64);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("body-bound load decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Logistics(
            LogisticsValidationError::PlayerBodyOutsideContinuousWorld,
        )))
    );
}

#[test]
fn ordinary_player_admission_rejects_partial_survival_setup() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("survival-only fixture failed: {error}"));

    assert_eq!(
        validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 0)).err(),
        Some(PlayerAdmissionError::PartialExistingAdmission {
            survival_initialized: true,
            logistics_initialized: false,
        })
    );
}

#[test]
fn ordinary_player_admission_rejects_partial_logistics_setup() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_player_logistics_initialization(&registries, &state, VoxelCoord::new(0, 0, 0))
        .unwrap_or_else(|error| panic!("logistics-only fixture failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("logistics-only fixture commit failed: {error}"));

    assert_eq!(
        validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 0)).err(),
        Some(PlayerAdmissionError::PartialExistingAdmission {
            survival_initialized: false,
            logistics_initialized: true,
        })
    );
}

#[test]
fn stale_inventory_rejects_player_admission_without_partial_commit() {
    let registries = build_registries();
    let mut state = AppState::new();
    let validated = validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 0))
        .unwrap_or_else(|error| panic!("player admission validation failed: {error}"));
    let _competitor = add_stockpile(
        &mut state,
        Mass::from_milligrams(1),
        StockpileStorageProfile::unbounded_solid_only(),
    )
    .unwrap_or_else(|error| panic!("competing inventory mutation failed: {error}"));
    let before_commit = state.clone();

    assert_eq!(
        validated.commit(&mut state),
        Err(PlayerAdmissionCommitError::StaleInventoryRevision {
            expected: 0,
            actual: 1,
        })
    );
    assert_eq!(state, before_commit);
}

#[test]
fn stockpile_allocation_preserves_inventory_revision_reserved_for_running_production() {
    let process = ProcessId::new(988_001);
    let registries = make_test_registries_with_standard_sensible_heating(process);
    let mut state = AppState::new();
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("allocation-headroom source failed: {error}"));
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("allocation-headroom destination failed: {error}"));
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(20),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("allocation-headroom source material failed: {error}"));
    let equipment = add_equipment(
        &registries,
        &mut state,
        STANDARD_TEST_HEATER,
        Condition::PRISTINE,
    )
    .unwrap_or_else(|error| panic!("allocation-headroom equipment failed: {error}"));
    let energy = add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        STANDARD_TEST_HEATING_ENERGY,
        Energy::from_nanojoules(10_000_000_000_000),
    )
    .unwrap_or_else(|error| panic!("allocation-headroom energy failed: {error}"));
    let resolved = resolve_sensible_heating_process(
        &registries,
        &state,
        SensibleHeatingRequest::new(
            process,
            source,
            &[MaterialLotSelection::new(lot, Mass::from_milligrams(10))],
            equipment,
            energy,
            Temperature::from_millikelvin(900_000),
        ),
    )
    .unwrap_or_else(|error| panic!("allocation-headroom process resolution failed: {error}"));
    validate_start_process(
        &registries,
        &state,
        resolved.process_resolution(),
        source,
        destination,
    )
    .unwrap_or_else(|error| panic!("allocation-headroom production start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("allocation-headroom production commit failed: {error}"));

    let required = state
        .checked_future_inventory_revision_demand()
        .unwrap_or_else(|| panic!("allocation-headroom future inventory demand overflowed"));
    assert!(
        required > 0,
        "running production must retain future inventory work"
    );
    let revision = u64::MAX - required;
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("allocation-headroom serialization failed: {error}"));
    encoded["state"]["systems"]["inventory"]["revision"] = serde_json::json!(revision);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("allocation-headroom decode failed: {error}"));
    let loaded = decoded.into_state(&registries).unwrap_or_else(|error| {
        panic!("exactly reserved inventory revision capacity should load: {error}")
    });
    let before = loaded.clone();

    assert_eq!(
        validate_allocate_ground_stockpile(
            &loaded,
            VoxelCoord::new(0, 0, 0),
            Mass::from_milligrams(1),
        )
        .err(),
        Some(GroundStockpileAllocationError::InventoryRevisionExhausted)
    );
    assert_eq!(loaded, before);
    assert_eq!(
        validate_admit_player(&registries, &loaded, VoxelCoord::new(0, 0, 0)).err(),
        Some(PlayerAdmissionError::InventoryRevisionExhausted)
    );
    assert_eq!(loaded, before);
}
