//! Tests for passive finite-store energy loss and owner revision semantics.

use super::*;
use crate::content::{ENERGY_THERMAL_SINK, build_registries};
use crate::core::quantity::Energy;
use crate::core::state::AppState;
use crate::core::time::WorldSeed;
use crate::energy::{
    add_energy_store_with_initial_for_test, apply_released_energy_outcomes, validate_energy_sink,
};

#[test]
fn passive_dissipation_removes_exact_pre_tick_energy_once() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xE930_0001));
    let initial = Energy::from_nanojoules(5_000_000_000_000_000);
    let store = add_energy_store_with_initial_for_test(
        &registries,
        &mut state,
        ENERGY_THERMAL_SINK,
        initial,
    )
    .unwrap_or_else(|error| panic!("thermal sink fixture failed: {error}"));
    let revision_before = state.energy().revision();

    let plan = decide_passive_energy_dissipation(&registries, &state);
    assert_eq!(plan.energy_revision_steps(), 1);
    apply_passive_energy_dissipation(&mut state, plan);

    assert_eq!(state.energy().revision(), revision_before + 1);
    assert_eq!(
        state
            .energy()
            .get_store(store)
            .map(|record| record.stored()),
        Some(Energy::from_nanojoules(1_400_000_000_000_000))
    );
}

#[test]
fn empty_dissipative_store_does_not_churn_energy_revision() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xE930_0002));
    add_energy_store_with_initial_for_test(
        &registries,
        &mut state,
        ENERGY_THERMAL_SINK,
        Energy::ZERO,
    )
    .unwrap_or_else(|error| panic!("empty thermal sink fixture failed: {error}"));
    let revision_before = state.energy().revision();

    let plan = decide_passive_energy_dissipation(&registries, &state);
    assert_eq!(plan.energy_revision_steps(), 0);
    apply_passive_energy_dissipation(&mut state, plan);

    assert_eq!(state.energy().revision(), revision_before);
}

#[test]
fn passive_dissipation_batches_multiple_stores_under_one_owner_revision() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xE930_0003));
    let first = add_energy_store_with_initial_for_test(
        &registries,
        &mut state,
        ENERGY_THERMAL_SINK,
        Energy::from_nanojoules(5_000_000_000_000_000),
    )
    .unwrap_or_else(|error| panic!("first thermal sink fixture failed: {error}"));
    let second = add_energy_store_with_initial_for_test(
        &registries,
        &mut state,
        ENERGY_THERMAL_SINK,
        Energy::from_nanojoules(1_000_000_000_000_000),
    )
    .unwrap_or_else(|error| panic!("second thermal sink fixture failed: {error}"));
    let revision_before = state.energy().revision();

    let plan = decide_passive_energy_dissipation(&registries, &state);
    assert_eq!(plan.energy_revision_steps(), 1);
    apply_passive_energy_dissipation(&mut state, plan);

    assert_eq!(state.energy().revision(), revision_before + 1);
    assert_eq!(
        state
            .energy()
            .get_store(first)
            .map(|record| record.stored()),
        Some(Energy::from_nanojoules(1_400_000_000_000_000))
    );
    assert_eq!(
        state
            .energy()
            .get_store(second)
            .map(|record| record.stored()),
        Some(Energy::ZERO)
    );
}

#[test]
fn same_tick_ingress_is_not_erased_by_pre_tick_passive_dissipation() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xE930_0004));
    let initial = Energy::from_nanojoules(1_000_000_000_000_000);
    let incoming = Energy::from_nanojoules(1_000_000_000_000_000);
    let store = add_energy_store_with_initial_for_test(
        &registries,
        &mut state,
        ENERGY_THERMAL_SINK,
        initial,
    )
    .unwrap_or_else(|error| panic!("same-tick passive-loss fixture failed: {error}"));

    let passive = decide_passive_energy_dissipation(&registries, &state);
    let ingress = validate_energy_sink(&registries, &state, store, incoming)
        .unwrap_or_else(|error| panic!("same-tick energy ingress validation failed: {error}"));
    let energy_revision = state.energy().revision();
    apply_released_energy_outcomes(
        state.energy_state_mut(),
        energy_revision,
        energy_revision + 1,
        &[ingress.trace()],
    );
    apply_passive_energy_dissipation(&mut state, passive);

    assert_eq!(
        state
            .energy()
            .get_store(store)
            .map(|record| record.stored()),
        Some(incoming),
        "passive loss must remove only energy that existed in the pre-tick snapshot"
    );
    assert_eq!(state.energy().revision(), energy_revision + 2);
}
