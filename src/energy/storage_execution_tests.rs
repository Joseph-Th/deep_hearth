//! Tests for the sibling storage execution module; isolated so test-only edits do not invalidate production builds.

use super::*;
use crate::content::make_test_registries_with_energy_store;
use crate::core::time::WorldSeed;

const STORE_DEFINITION: EnergyStoreDefinitionId = EnergyStoreDefinitionId::new(930_001);

fn registries() -> Registries {
    make_test_registries_with_energy_store(super::super::EnergyStoreDefinition::new(
        STORE_DEFINITION,
        "energy execution fixture",
        EnergyCarrier::Electrical,
        Energy::from_nanojoules(1_000),
        Power::from_microwatts(25),
    ))
}

fn sink_registries() -> Registries {
    make_test_registries_with_energy_store(
        super::super::EnergyStoreDefinition::new_with_transfer_limits(
            STORE_DEFINITION,
            "energy sink execution fixture",
            EnergyCarrier::Thermal,
            Energy::from_nanojoules(1_000),
            Power::from_microwatts(40),
            Power::ZERO,
        ),
    )
}

#[test]
fn allocation_rejects_energy_above_authored_capacity_without_mutation() {
    let registries = registries();
    let mut state = AppState::new(WorldSeed::new(0x9300_0001));
    let before = state.clone();

    assert_eq!(
        add_energy_store_with_initial_for_fixture(
            &registries,
            &mut state,
            STORE_DEFINITION,
            Energy::from_nanojoules(1_001),
        ),
        Err(AddEnergyStoreError::InitialEnergyExceedsCapacity {
            initial: Energy::from_nanojoules(1_001),
            capacity: Energy::from_nanojoules(1_000),
        })
    );
    assert_eq!(state, before);
}

#[test]
fn runtime_allocation_creates_empty_store_without_free_energy() {
    let registries = registries();
    let mut state = AppState::new(WorldSeed::new(0x9300_0005));

    let store = match add_energy_store(&registries, &mut state, STORE_DEFINITION) {
        Ok(store) => store,
        Err(error) => panic!("runtime energy-store allocation failed: {error}"),
    };

    assert_eq!(
        state
            .energy()
            .get_store(store)
            .map(EnergyStoreRecord::stored),
        Some(Energy::ZERO)
    );
    assert_eq!(state.energy().revision(), 1);
}

#[test]
fn validated_supply_consumes_exact_energy_and_preserves_trace() {
    let registries = registries();
    let mut state = AppState::new(WorldSeed::new(0x9300_0002));
    let store = match add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        STORE_DEFINITION,
        Energy::from_nanojoules(900),
    ) {
        Ok(store) => store,
        Err(error) => panic!("energy store fixture failed: {error}"),
    };
    let supply =
        match validate_energy_supply(&registries, &state, store, Energy::from_nanojoules(275)) {
            Ok(supply) => supply,
            Err(error) => panic!("energy supply validation failed: {error}"),
        };
    assert_eq!(supply.max_output_power(), Power::from_microwatts(25));
    let reservation = match validate_energy_consumption_reservation(state.energy(), supply) {
        Ok(reservation) => reservation,
        Err(error) => panic!("energy reservation failed: {error:?}"),
    };
    let trace = match apply_energy_consumption_reservation(state.energy_state_mut(), reservation) {
        Ok(trace) => trace,
        Err(error) => panic!("energy consumption commit failed: {error:?}"),
    };

    assert_eq!(trace.source(), store);
    assert_eq!(trace.definition(), STORE_DEFINITION);
    assert_eq!(trace.carrier(), EnergyCarrier::Electrical);
    assert_eq!(trace.energy(), Energy::from_nanojoules(275));
    assert_eq!(
        state
            .energy()
            .get_store(store)
            .map(EnergyStoreRecord::stored),
        Some(Energy::from_nanojoules(625))
    );
    assert_eq!(state.energy().revision(), 2);
}

#[test]
fn stale_supply_is_rejected_after_independent_energy_mutation() {
    let registries = registries();
    let mut state = AppState::new(WorldSeed::new(0x9300_0003));
    let store = match add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        STORE_DEFINITION,
        Energy::from_nanojoules(900),
    ) {
        Ok(store) => store,
        Err(error) => panic!("energy store fixture failed: {error}"),
    };
    let supply =
        match validate_energy_supply(&registries, &state, store, Energy::from_nanojoules(100)) {
            Ok(supply) => supply,
            Err(error) => panic!("energy supply validation failed: {error}"),
        };
    let expected = state.energy().revision();
    if let Err(error) = add_energy_store(&registries, &mut state, STORE_DEFINITION) {
        panic!("independent energy allocation failed: {error}");
    }
    let before = state.clone();

    assert_eq!(
        validate_energy_consumption_reservation(state.energy(), supply),
        Err(EnergyReservationError::StaleSelection {
            expected,
            actual: expected + 1,
        })
    );
    assert_eq!(state, before);
}

#[test]
fn supply_rejects_insufficient_energy_without_mutation() {
    let registries = registries();
    let mut state = AppState::new(WorldSeed::new(0x9300_0004));
    let store = match add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        STORE_DEFINITION,
        Energy::from_nanojoules(50),
    ) {
        Ok(store) => store,
        Err(error) => panic!("energy store fixture failed: {error}"),
    };
    let before = state.clone();

    assert_eq!(
        validate_energy_supply(&registries, &state, store, Energy::from_nanojoules(51),),
        Err(EnergySupplyError::InsufficientEnergy {
            store,
            available: Energy::from_nanojoules(50),
            requested: Energy::from_nanojoules(51),
        })
    );
    assert_eq!(state, before);
}

#[test]
fn output_only_store_rejects_energy_sink_binding_without_mutation() {
    let registries = registries();
    let mut state = AppState::new(WorldSeed::new(0x9300_0006));
    let store = match add_energy_store(&registries, &mut state, STORE_DEFINITION) {
        Ok(store) => store,
        Err(error) => panic!("output-only store fixture failed: {error}"),
    };
    let before = state.clone();

    assert_eq!(
        validate_energy_sink(&registries, &state, store, Energy::from_nanojoules(1),),
        Err(EnergySinkError::NoInputPower { store })
    );
    assert_eq!(state, before);
}

#[test]
fn sink_only_store_rejects_energy_supply_binding_without_mutation() {
    let registries = sink_registries();
    let mut state = AppState::new(WorldSeed::new(0x9300_0007));
    let store = match add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        STORE_DEFINITION,
        Energy::from_nanojoules(500),
    ) {
        Ok(store) => store,
        Err(error) => panic!("sink-only store fixture failed: {error}"),
    };
    let before = state.clone();

    assert_eq!(
        validate_energy_supply(&registries, &state, store, Energy::from_nanojoules(1),),
        Err(EnergySupplyError::NoOutputPower { store })
    );
    assert_eq!(state, before);
}

#[test]
fn sink_binding_reserves_exact_capacity_and_is_revision_bound() {
    let registries = sink_registries();
    let mut state = AppState::new(WorldSeed::new(0x9300_0008));
    let store = match add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        STORE_DEFINITION,
        Energy::from_nanojoules(700),
    ) {
        Ok(store) => store,
        Err(error) => panic!("energy sink fixture failed: {error}"),
    };
    let sink = match validate_energy_sink(&registries, &state, store, Energy::from_nanojoules(300))
    {
        Ok(sink) => sink,
        Err(error) => panic!("energy sink validation failed: {error}"),
    };
    assert_eq!(sink.max_input_power(), Power::from_microwatts(40));
    assert_eq!(sink.trace().destination(), store);
    assert_eq!(sink.trace().energy(), Energy::from_nanojoules(300));
    assert_eq!(
        state
            .energy()
            .get_store(store)
            .map(EnergyStoreRecord::stored),
        Some(Energy::from_nanojoules(700))
    );

    let expected = state.energy().revision();
    if let Err(error) = add_energy_store(&registries, &mut state, STORE_DEFINITION) {
        panic!("independent sink mutation failed: {error}");
    }
    assert_eq!(
        validate_energy_ingress_reservation(&registries, state.energy(), sink),
        Err(EnergyIngressReservationError::StaleSelection {
            expected,
            actual: expected + 1,
        })
    );
}

#[test]
fn sink_rejects_capacity_overrun_without_mutation() {
    let registries = sink_registries();
    let mut state = AppState::new(WorldSeed::new(0x9300_0009));
    let store = match add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        STORE_DEFINITION,
        Energy::from_nanojoules(900),
    ) {
        Ok(store) => store,
        Err(error) => panic!("capacity sink fixture failed: {error}"),
    };
    let before = state.clone();

    assert_eq!(
        validate_energy_sink(&registries, &state, store, Energy::from_nanojoules(101),),
        Err(EnergySinkError::InsufficientCapacity {
            store,
            stored: Energy::from_nanojoules(900),
            requested: Energy::from_nanojoules(101),
            capacity: Energy::from_nanojoules(1_000),
        })
    );
    assert_eq!(state, before);
}
