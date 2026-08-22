//! Tests for the sibling transfer execution module; isolated so test-only edits do not invalidate production builds.

use super::*;
use crate::content::{
    FORM_LOG, MATERIAL_WOOD, make_test_registries_with_energy_stores,
    make_test_registries_with_energy_stores_and_process,
};
use crate::core::quantity::{Mass, Power, Temperature};

#[cfg(feature = "test-soak")]
use crate::core::state::validate_loaded_state;
use crate::core::time::WorldSeed;
use crate::energy::{
    EnergyStoreDefinition, EnergyStoreDefinitionId, add_energy_store,
    add_energy_store_with_initial_for_test,
};
use crate::inventory::{add_solid_stockpile_for_test, deposit_bulk_for_test};
use crate::material::{CommodityKey, MaterialInputSpec, MaterialLotSpec};
use crate::production::{
    ProcessDefinition, ProcessId, make_test_process_resolution, validate_process_inputs,
    validate_start_process,
};

const ELECTRICAL_STORE: EnergyStoreDefinitionId = EnergyStoreDefinitionId::new(931_001);
const THERMAL_STORE: EnergyStoreDefinitionId = EnergyStoreDefinitionId::new(931_002);
const TEST_PROCESS: ProcessId = ProcessId::new(931_001);

fn bidirectional_definition(
    id: EnergyStoreDefinitionId,
    carrier: EnergyCarrier,
) -> EnergyStoreDefinition {
    EnergyStoreDefinition::new_with_transfer_limits(
        id,
        "energy transfer fixture",
        carrier,
        Energy::from_nanojoules(1_000),
        Power::from_microwatts(20),
        Power::from_microwatts(25),
    )
}

#[cfg(feature = "test-soak")]
fn stored_energy_total(state: &AppState) -> Energy {
    match state
        .energy()
        .stores()
        .try_fold(Energy::ZERO, |total, store| {
            total.checked_add(store.stored())
        }) {
        Some(total) => total,
        None => panic!("energy transfer fixture total overflowed authoritative accounting"),
    }
}

#[cfg(feature = "test-soak")]
fn run_transfer_soak(seed: WorldSeed) -> AppState {
    let registries = registries();
    let mut state = AppState::new(seed);
    let left = match add_energy_store_with_initial_for_test(
        &registries,
        &mut state,
        ELECTRICAL_STORE,
        Energy::from_nanojoules(600),
    ) {
        Ok(store) => store,
        Err(error) => panic!("energy soak left-store fixture failed: {error}"),
    };
    let right = match add_energy_store_with_initial_for_test(
        &registries,
        &mut state,
        ELECTRICAL_STORE,
        Energy::from_nanojoules(400),
    ) {
        Ok(store) => store,
        Err(error) => panic!("energy soak right-store fixture failed: {error}"),
    };
    let initial_total = stored_energy_total(&state);

    for step in 0..2_000_u64 {
        let (source, destination) = if step.is_multiple_of(2) {
            (left, right)
        } else {
            (right, left)
        };
        let resolution =
            make_test_energy_transfer_resolution(source, destination, Energy::from_nanojoules(1));
        let validated = match validate_energy_transfer(&registries, &state, resolution) {
            Ok(validated) => validated,
            Err(error) => panic!("energy soak validation failed at step {step}: {error}"),
        };
        if let Err(error) = validated.commit(&mut state) {
            panic!("energy soak commit failed at step {step}: {error}");
        }

        if step.is_multiple_of(137) {
            assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
            assert_eq!(stored_energy_total(&state), initial_total);
        }
    }

    assert_eq!(stored_energy_total(&state), initial_total);
    assert_eq!(
        state
            .energy()
            .get_store(left)
            .map(EnergyStoreRecord::stored),
        Some(Energy::from_nanojoules(600))
    );
    assert_eq!(
        state
            .energy()
            .get_store(right)
            .map(EnergyStoreRecord::stored),
        Some(Energy::from_nanojoules(400))
    );
    state
}

fn registries() -> Registries {
    make_test_registries_with_energy_stores(vec![bidirectional_definition(
        ELECTRICAL_STORE,
        EnergyCarrier::Electrical,
    )])
}

fn no_energy_process() -> ProcessDefinition {
    ProcessDefinition::new(
        TEST_PROCESS,
        "energy transfer production-revision fixture",
        vec![MaterialInputSpec::new(
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(1),
        )],
        Vec::new(),
    )
}

#[test]
fn validated_transfer_conserves_energy_and_advances_revision_once() {
    let registries = registries();
    let mut state = AppState::new(WorldSeed::new(0x9310_0001));
    let source = match add_energy_store_with_initial_for_test(
        &registries,
        &mut state,
        ELECTRICAL_STORE,
        Energy::from_nanojoules(700),
    ) {
        Ok(store) => store,
        Err(error) => panic!("source energy fixture failed: {error}"),
    };
    let destination = match add_energy_store_with_initial_for_test(
        &registries,
        &mut state,
        ELECTRICAL_STORE,
        Energy::from_nanojoules(100),
    ) {
        Ok(store) => store,
        Err(error) => panic!("destination energy fixture failed: {error}"),
    };
    let revision_before = state.energy().revision();
    let resolution =
        make_test_energy_transfer_resolution(source, destination, Energy::from_nanojoules(250));
    let validated = match validate_energy_transfer(&registries, &state, resolution) {
        Ok(validated) => validated,
        Err(error) => panic!("energy transfer validation failed: {error}"),
    };
    let outcome = match validated.commit(&mut state) {
        Ok(outcome) => outcome,
        Err(error) => panic!("energy transfer commit failed: {error}"),
    };

    assert_eq!(outcome.source(), source);
    assert_eq!(outcome.destination(), destination);
    assert_eq!(outcome.carrier(), EnergyCarrier::Electrical);
    assert_eq!(outcome.energy(), Energy::from_nanojoules(250));
    assert_eq!(
        state
            .energy()
            .get_store(source)
            .map(EnergyStoreRecord::stored),
        Some(Energy::from_nanojoules(450))
    );
    assert_eq!(
        state
            .energy()
            .get_store(destination)
            .map(EnergyStoreRecord::stored),
        Some(Energy::from_nanojoules(350))
    );
    let total = state
        .energy()
        .stores()
        .try_fold(Energy::ZERO, |total, store| {
            total.checked_add(store.stored())
        });
    assert_eq!(total, Some(Energy::from_nanojoules(800)));
    assert_eq!(state.energy().revision(), revision_before + 1);
}

#[test]
fn storage_boundary_rejects_implicit_carrier_conversion_without_mutation() {
    let registries = make_test_registries_with_energy_stores(vec![
        bidirectional_definition(ELECTRICAL_STORE, EnergyCarrier::Electrical),
        bidirectional_definition(THERMAL_STORE, EnergyCarrier::Thermal),
    ]);
    let mut state = AppState::new(WorldSeed::new(0x9310_0002));
    let source = match add_energy_store_with_initial_for_test(
        &registries,
        &mut state,
        ELECTRICAL_STORE,
        Energy::from_nanojoules(500),
    ) {
        Ok(store) => store,
        Err(error) => panic!("electrical source fixture failed: {error}"),
    };
    let destination = match add_energy_store(&registries, &mut state, THERMAL_STORE) {
        Ok(store) => store,
        Err(error) => panic!("thermal destination fixture failed: {error}"),
    };
    let before = state.clone();

    assert_eq!(
        validate_energy_transfer(
            &registries,
            &state,
            make_test_energy_transfer_resolution(source, destination, Energy::from_nanojoules(1),),
        ),
        Err(EnergyTransferError::CarrierMismatch {
            source: EnergyCarrier::Electrical,
            destination: EnergyCarrier::Thermal,
        })
    );
    assert_eq!(state, before);
}

#[test]
fn transfer_capacity_failure_is_atomic() {
    let registries = registries();
    let mut state = AppState::new(WorldSeed::new(0x9310_0003));
    let source = match add_energy_store_with_initial_for_test(
        &registries,
        &mut state,
        ELECTRICAL_STORE,
        Energy::from_nanojoules(500),
    ) {
        Ok(store) => store,
        Err(error) => panic!("capacity source fixture failed: {error}"),
    };
    let destination = match add_energy_store_with_initial_for_test(
        &registries,
        &mut state,
        ELECTRICAL_STORE,
        Energy::from_nanojoules(900),
    ) {
        Ok(store) => store,
        Err(error) => panic!("capacity destination fixture failed: {error}"),
    };
    let before = state.clone();

    assert_eq!(
        validate_energy_transfer(
            &registries,
            &state,
            make_test_energy_transfer_resolution(source, destination, Energy::from_nanojoules(101),),
        ),
        Err(EnergyTransferError::DestinationCapacityExceeded {
            store: destination,
            stored: Energy::from_nanojoules(900),
            requested: Energy::from_nanojoules(101),
            capacity: Energy::from_nanojoules(1_000),
        })
    );
    assert_eq!(state, before);
}

#[test]
fn stale_energy_revision_rejects_validated_transfer_without_partial_mutation() {
    let registries = registries();
    let mut state = AppState::new(WorldSeed::new(0x9310_0004));
    let source = match add_energy_store_with_initial_for_test(
        &registries,
        &mut state,
        ELECTRICAL_STORE,
        Energy::from_nanojoules(500),
    ) {
        Ok(store) => store,
        Err(error) => panic!("stale source fixture failed: {error}"),
    };
    let destination = match add_energy_store(&registries, &mut state, ELECTRICAL_STORE) {
        Ok(store) => store,
        Err(error) => panic!("stale destination fixture failed: {error}"),
    };
    let validated = match validate_energy_transfer(
        &registries,
        &state,
        make_test_energy_transfer_resolution(source, destination, Energy::from_nanojoules(100)),
    ) {
        Ok(validated) => validated,
        Err(error) => panic!("stale transfer validation failed: {error}"),
    };
    let expected = state.energy().revision();
    if let Err(error) = add_energy_store(&registries, &mut state, ELECTRICAL_STORE) {
        panic!("independent energy mutation failed: {error}");
    }
    let before_commit = state.clone();

    assert_eq!(
        validated.commit(&mut state),
        Err(EnergyTransferCommitError::StaleEnergyRevision {
            expected,
            actual: expected + 1,
        })
    );
    assert_eq!(state, before_commit);
}

#[test]
fn stale_production_revision_rejects_validated_transfer_without_partial_mutation() {
    let registries = make_test_registries_with_energy_stores_and_process(
        vec![bidirectional_definition(
            ELECTRICAL_STORE,
            EnergyCarrier::Electrical,
        )],
        no_energy_process(),
    );
    let mut state = AppState::new(WorldSeed::new(0x9310_0005));
    let source = match add_energy_store_with_initial_for_test(
        &registries,
        &mut state,
        ELECTRICAL_STORE,
        Energy::from_nanojoules(500),
    ) {
        Ok(store) => store,
        Err(error) => panic!("production-stale source fixture failed: {error}"),
    };
    let destination = match add_energy_store(&registries, &mut state, ELECTRICAL_STORE) {
        Ok(store) => store,
        Err(error) => panic!("production-stale destination fixture failed: {error}"),
    };
    let validated = match validate_energy_transfer(
        &registries,
        &state,
        make_test_energy_transfer_resolution(source, destination, Energy::from_nanojoules(100)),
    ) {
        Ok(validated) => validated,
        Err(error) => panic!("production-stale transfer validation failed: {error}"),
    };
    let expected_energy_revision = state.energy().revision();
    let expected_production_revision = state.production().revision();

    let material_source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2)) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("production-stale material source failed: {error}"),
    };
    let material_destination =
        match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2)) {
            Ok(stockpile) => stockpile,
            Err(error) => panic!("production-stale material destination failed: {error}"),
        };
    if let Err(error) = deposit_bulk_for_test(
        &registries,
        &mut state,
        material_source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(1),
    ) {
        panic!("production-stale material seeding failed: {error}");
    }
    let inputs = match validate_process_inputs(&registries, &state, TEST_PROCESS, material_source) {
        Ok(inputs) => inputs,
        Err(error) => panic!("production-stale process input binding failed: {error}"),
    };
    let resolution = make_test_process_resolution(
        inputs,
        2,
        vec![MaterialLotSpec::new(
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(1),
            Temperature::from_millikelvin(293_150),
        )],
    );
    let start = match validate_start_process(
        &registries,
        &state,
        &resolution,
        material_source,
        material_destination,
    ) {
        Ok(start) => start,
        Err(error) => panic!("production-stale process start validation failed: {error}"),
    };
    if let Err(error) = start.commit(&mut state) {
        panic!("production-stale process start commit failed: {error}");
    }
    assert_eq!(state.energy().revision(), expected_energy_revision);
    assert_eq!(
        state.production().revision(),
        expected_production_revision + 1
    );
    let before_commit = state.clone();

    assert_eq!(
        validated.commit(&mut state),
        Err(EnergyTransferCommitError::StaleProductionRevision {
            expected: expected_production_revision,
            actual: expected_production_revision + 1,
        })
    );
    assert_eq!(state, before_commit);
}

#[cfg(feature = "test-soak")]
#[test]
#[ignore = "long-horizon soak"]
fn energy_transfer_soak_preserves_conservation_audits_and_replay() {
    let seed = WorldSeed::new(0x9310_0006);
    let first = run_transfer_soak(seed);
    let second = run_transfer_soak(seed);
    assert_eq!(first, second);
}
