//! Tests for the sibling mod module; isolated so test-only edits do not invalidate production builds.

use super::*;
use crate::content::build_registries;
use crate::core::state::make_test_state_at_tick;
use crate::core::time::WorldSeed;
use crate::survival::{Vitality, initialize_player_survival, player_record};

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
fn clock_exhaustion_leaves_state_unchanged() {
    let registries = build_registries();
    let mut state = make_test_state_at_tick(WorldSeed::new(9), SimulationTick::new(u64::MAX));
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
