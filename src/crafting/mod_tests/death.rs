//! Fatal-tick manual-production suspension and due-completion contracts.

use super::*;

#[test]
fn fatal_tick_suspends_unfinished_manual_craft_and_preserves_work_in_process() {
    let (registries, mut state, source, lot, destination) = make_fixture();
    let job = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(
            PROCESS_KNAP_STONE_TOOL,
            source,
            MaterialLotSelection::new(lot, Mass::from_milligrams(1_000_000)),
            destination,
        ),
    )
    .unwrap_or_else(|error| panic!("fatal manual craft start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("fatal manual craft start commit failed: {error}"));
    let active = state
        .production()
        .get_job(job)
        .unwrap_or_else(|| panic!("fatal manual craft job disappeared after start"));
    assert!(
        active.completes_at().value() > state.tick().value() + 1,
        "fatal manual craft proof requires unfinished work after the next tick"
    );
    let reserved_before = state
        .inventory()
        .get_stockpile(destination)
        .map(|stockpile| stockpile.reserved_inbound())
        .unwrap_or_else(|| panic!("fatal manual craft destination disappeared"));
    assert!(!reserved_before.is_zero());
    make_next_tick_fatal(&registries, &mut state);

    let outcome = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("fatal manual craft tick failed: {error}"));

    assert!(outcome.production_completions().is_empty());
    assert!(matches!(
        outcome.production_availability_changes(),
        [ProductionAvailabilityChange::Suspended {
            job: suspended_job,
            reason: ProductionSuspensionReason::PlayerLaborUnavailable,
            ..
        }] if *suspended_job == job
    ));
    assert_eq!(state.player_work().active(), None);
    assert_eq!(
        state.survival().player().map(|player| player.vitality()),
        Some(Vitality::ZERO)
    );
    assert_eq!(
        state
            .production()
            .get_job(job)
            .and_then(|record| record.suspension())
            .map(|suspension| suspension.reason()),
        Some(ProductionSuspensionReason::PlayerLaborUnavailable)
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|stockpile| stockpile.reserved_inbound()),
        Some(reserved_before),
        "suspended manual production must retain its reserved output capacity"
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::ZERO),
        "unfinished manual production must not materialize output on the fatal tick"
    );
    validate_loaded_state(&registries, &state).unwrap_or_else(|error| {
        panic!("fatal manual craft state failed trusted-load audit: {error}")
    });
}

#[test]
fn fatal_tick_allows_manual_craft_due_that_tick_to_complete() {
    let (registries, mut state, source, lot, destination) = make_fixture();
    let job = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(
            PROCESS_KNAP_STONE_TOOL,
            source,
            MaterialLotSelection::new(lot, Mass::from_milligrams(1_000_000)),
            destination,
        ),
    )
    .unwrap_or_else(|error| panic!("fatal due manual craft start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("fatal due manual craft start commit failed: {error}"));
    let completes_at = state
        .production()
        .get_job(job)
        .map(|record| record.completes_at())
        .unwrap_or_else(|| panic!("fatal due manual craft job disappeared after start"));
    while state.tick().value() + 1 < completes_at.value() {
        let outcome = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("fatal due manual craft setup tick failed: {error}"));
        assert!(outcome.production_completions().is_empty());
    }
    make_next_tick_fatal(&registries, &mut state);

    let outcome = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("fatal due manual craft completion tick failed: {error}"));

    assert_eq!(outcome.production_completions().len(), 1);
    assert_eq!(outcome.production_completions()[0].job(), job);
    assert!(state.production().get_job(job).is_none());
    assert_eq!(state.player_work().active(), None);
    assert_eq!(
        state.survival().player().map(|player| player.vitality()),
        Some(Vitality::ZERO)
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|stockpile| stockpile.reserved_inbound()),
        Some(Mass::ZERO)
    );
    assert!(
        state
            .inventory()
            .get_stockpile(destination)
            .is_some_and(|stockpile| !stockpile.stored_mass().is_zero()),
        "manual production due on the fatal tick must materialize its output"
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("fatal due manual craft state invalid: {error}"));
}
