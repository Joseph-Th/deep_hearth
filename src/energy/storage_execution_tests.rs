//! Contract tests for finite energy-store execution.

use super::*;
use crate::content::make_test_registries_with_energy_store;
use crate::core::quantity::{Energy, Power};
use crate::core::state::AppState;
use crate::core::time::{TickSpan, WorldSeed};
use crate::energy::{
    EnergyCarrier, EnergyStoreDefinitionId, EnergyStoreRecord, add_energy_store,
    add_energy_store_with_initial_for_fixture,
};
use crate::registry::Registries;

const STORE_DEFINITION: EnergyStoreDefinitionId = EnergyStoreDefinitionId::new(930_001);

fn registries() -> Registries {
    make_test_registries_with_energy_store(
        super::super::EnergyStoreDefinition::new_with_transfer_limits(
            STORE_DEFINITION,
            "energy execution fixture",
            EnergyCarrier::Electrical,
            Energy::from_nanojoules(1_000),
            Power::ZERO,
            Power::from_microwatts(25),
        ),
    )
}

fn dissipative_sink_registries() -> Registries {
    make_test_registries_with_energy_store(
        super::super::EnergyStoreDefinition::new_with_transfer_limits(
            STORE_DEFINITION,
            "dissipative energy sink execution fixture",
            EnergyCarrier::Thermal,
            Energy::from_nanojoules(10_000),
            Power::from_microwatts(40),
            Power::ZERO,
        )
        .with_passive_dissipation_power(Power::from_picowatts(250_000)),
    )
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
fn deferred_sink_capacity_counts_only_passive_ticks_before_completion_ingress() {
    let registries = dissipative_sink_registries();
    let mut state = AppState::new(WorldSeed::new(0x9300_0010));
    let store = add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        STORE_DEFINITION,
        Energy::from_nanojoules(2_000),
    )
    .unwrap_or_else(|error| panic!("dissipative sink fixture failed: {error}"));
    let access = validate_energy_sink_access(&registries, &state, store)
        .unwrap_or_else(|error| panic!("dissipative sink access failed: {error}"));

    assert_eq!(
        project_energy_sink_stored_at_release(
            &registries,
            STORE_DEFINITION,
            Energy::from_nanojoules(2_000),
            TickSpan::new(1),
        ),
        Energy::from_nanojoules(2_000),
        "a one-tick job releases before that tick's passive loss"
    );
    assert_eq!(
        project_energy_sink_stored_at_release(
            &registries,
            STORE_DEFINITION,
            Energy::from_nanojoules(2_000),
            TickSpan::new(2),
        ),
        Energy::from_nanojoules(1_100),
        "a two-tick job has exactly one guaranteed passive-loss tick before ingress"
    );
    assert_eq!(
        validate_energy_sink_release(
            &registries,
            access,
            Energy::from_nanojoules(9_000),
            TickSpan::new(2),
        ),
        Err(EnergySinkError::InsufficientCapacity {
            store,
            stored: Energy::from_nanojoules(1_100),
            requested: Energy::from_nanojoules(9_000),
            capacity: Energy::from_nanojoules(10_000),
        }),
        "completion-tick dissipation must not be credited before the release is committed"
    );
    assert!(
        validate_energy_sink_release(
            &registries,
            access,
            Energy::from_nanojoules(9_000),
            TickSpan::new(3),
        )
        .is_ok(),
        "a third tick adds a second pre-release passive-loss interval and makes the release fit"
    );
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
        validate_energy_ingress_reservation(&registries, state.energy(), sink, TickSpan::new(0),),
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
