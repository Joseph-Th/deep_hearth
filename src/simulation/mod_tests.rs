//! Contract tests for canonical simulation-tick orchestration.

use super::*;
use crate::content::{build_registries, make_test_registries_with_energy_store};
use crate::core::quantity::{Energy, Power};
use crate::core::state::apply_clock_advance;
use crate::core::time::WorldSeed;
use crate::energy::{
    EnergyCarrier, EnergyStoreDefinition, EnergyStoreDefinitionId,
    add_energy_store_with_initial_for_fixture,
};
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::registry::Registries;
use crate::survival::{Vitality, initialize_player_survival, player_record};

const DISSIPATIVE_STORE: EnergyStoreDefinitionId = EnergyStoreDefinitionId::new(510_001);

fn passive_dissipation_registries() -> Registries {
    make_test_registries_with_energy_store(
        EnergyStoreDefinition::new_with_transfer_limits(
            DISSIPATIVE_STORE,
            "simulation passive dissipation fixture",
            EnergyCarrier::Thermal,
            Energy::from_nanojoules(10_000_000_000_000_000),
            Power::from_microwatts(1_000_000_000_000),
            Power::ZERO,
        )
        .with_passive_dissipation_power(Power::from_microwatts(1_000_000_000_000)),
    )
}

#[test]
fn canonical_tick_advances_exactly_once() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(7));

    let result = advance_tick(&registries, &mut state);
    let outcome = match result {
        Ok(outcome) => outcome,
        Err(error) => panic!("tick unexpectedly failed: {error}"),
    };

    assert_eq!(outcome.tick(), SimulationTick::new(1));
    assert_eq!(state.tick(), SimulationTick::new(1));
}

#[test]
fn canonical_tick_applies_exact_passive_energy_dissipation() {
    let registries = passive_dissipation_registries();
    let mut state = AppState::new(WorldSeed::new(0x5100_0001));
    let store = add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        DISSIPATIVE_STORE,
        Energy::from_nanojoules(5_000_000_000_000_000),
    )
    .unwrap_or_else(|error| panic!("passive-dissipation tick fixture failed: {error}"));

    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("passive-dissipation tick failed: {error}"));

    assert_eq!(
        state
            .energy()
            .get_store(store)
            .map(|record| record.stored()),
        Some(Energy::from_nanojoules(1_400_000_000_000_000))
    );
}

#[test]
fn clock_exhaustion_leaves_state_unchanged() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(9));
    apply_clock_advance(&mut state, SimulationTick::new(u64::MAX));
    let before = state.clone();

    let result = advance_tick(&registries, &mut state);

    assert_eq!(
        result,
        Err(TickError::ClockExhausted {
            current: SimulationTick::new(u64::MAX),
        })
    );
    assert_eq!(state, before);
}

#[test]
fn exhausted_survival_revision_reloaded_from_save_rejects_tick_atomically() {
    let registries = build_registries();
    let mut source = AppState::new(WorldSeed::new(0x5100_000A));
    initialize_player_survival(&registries, &mut source)
        .unwrap_or_else(|error| panic!("survival-revision fixture failed: {error}"));
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &source))
        .unwrap_or_else(|error| panic!("survival-revision fixture serialization failed: {error}"));
    encoded["state"]["systems"]["survival"]["revision"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("survival-revision fixture decode failed: {error}"));
    let mut state = decoded.into_state(&registries).unwrap_or_else(|error| {
        panic!("exhausted survival revision should remain load-valid: {error}")
    });
    let before = state.clone();

    assert_eq!(
        advance_tick(&registries, &mut state),
        Err(TickError::SurvivalRevisionExhausted)
    );
    assert_eq!(state, before);
}

#[test]
fn exhausted_energy_revision_reloaded_from_save_rejects_passive_tick_atomically() {
    let registries = passive_dissipation_registries();
    let mut source = AppState::new(WorldSeed::new(0x5100_000B));
    let _store = add_energy_store_with_initial_for_fixture(
        &registries,
        &mut source,
        DISSIPATIVE_STORE,
        Energy::from_nanojoules(5_000_000_000_000_000),
    )
    .unwrap_or_else(|error| panic!("energy-revision fixture failed: {error}"));
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &source))
        .unwrap_or_else(|error| panic!("energy-revision fixture serialization failed: {error}"));
    encoded["state"]["systems"]["energy"]["revision"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("energy-revision fixture decode failed: {error}"));
    let mut state = decoded.into_state(&registries).unwrap_or_else(|error| {
        panic!("exhausted energy revision should remain load-valid: {error}")
    });
    let before = state.clone();

    assert_eq!(
        advance_tick(&registries, &mut state),
        Err(TickError::EnergyRevisionExhausted)
    );
    assert_eq!(state, before);
}

#[test]
fn shared_owner_revision_capacity_accounts_for_all_same_tick_mutations() {
    assert!(has_revision_capacity(u64::MAX - 2, 2));
    assert!(!has_revision_capacity(u64::MAX - 2, 3));
    assert!(!has_revision_capacity(u64::MAX - 1, 2));
}

#[test]
fn dead_player_remains_visible_in_tick_survival_outcome_without_mutation() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5100_1001));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("dead-player survival initialization failed: {error}"));
    let player = state
        .survival()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("initialized dead-player fixture is missing survival state"));
    let expected_revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        expected_revision,
        expected_revision + 1,
        player_record(
            player.metabolic_energy(),
            player.hydration(),
            Vitality::ZERO,
            player.nutrition(),
            player.vitality_recovery_remainder(),
        ),
    );
    let frozen_revision = state.survival().revision();
    let frozen_player = state.survival().player().copied();

    let outcome = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("dead-player tick failed: {error}"));

    assert_eq!(
        outcome.survival().map(SurvivalAssessment::vitality),
        Some(Vitality::ZERO)
    );
    assert_eq!(state.survival().revision(), frozen_revision);
    assert_eq!(state.survival().player().copied(), frozen_player);
}
