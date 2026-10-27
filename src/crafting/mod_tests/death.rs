//! Fatal-tick manual-production suspension and due-completion contracts.

use super::*;

#[test]
fn fatal_tick_does_not_restart_a_suspended_manual_craft() {
    use crate::survival::player_record_with_thermal_comfort;

    let (registries, mut state, source, lot, destination) = make_fixture();
    let support = active_stockpile_support(&registries, &mut state);
    let _ = validate_mount_stockpile(&registries, &state, destination, support)
        .unwrap()
        .commit(&mut state)
        .unwrap();
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
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let _ = validate_set_structural_load(
        &registries,
        &state,
        support,
        StructuralLoadKind::Snow,
        Force::from_millinewtons(50_000_000),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let paused = advance_tick(&registries, &mut state).unwrap();
    assert!(matches!(
        paused.production_availability_changes(),
        [ProductionAvailabilityChange::Suspended { job: stopped, .. }] if *stopped == job
    ));
    assert_eq!(state.player_work().active(), None);
    let _ = validate_unmount_stockpile(&registries, &state, destination)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let suspension_before = state.production().get_job(job).unwrap().suspension();
    let reserved_before = state
        .inventory()
        .get_stockpile(destination)
        .unwrap()
        .reserved_inbound();
    let matter_before = calculate_matter_accounting(&state).unwrap().total();

    // This player has the energy and hydration to begin the interrupted work,
    // but no thermal reserve or vitality to survive the current outdoor tick.
    // Production's pre-tick availability forecast therefore sees labor that
    // cannot remain available after the authoritative survival settlement.
    let before = state.survival().player().copied().unwrap();
    let revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        revision,
        revision + 1,
        player_record_with_thermal_comfort(
            before.metabolic_energy(),
            before.hydration(),
            Vitality::from_parts_per_million_unchecked(1),
            before.nutrition(),
            before.vitality_recovery_remainder(),
            0,
        ),
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let outcome = advance_tick(&registries, &mut state).unwrap();
    assert!(outcome.production_completions().is_empty());
    assert!(
        !outcome.production_availability_changes().iter().any(|change| {
            matches!(change, ProductionAvailabilityChange::Resumed { job: resumed, .. } if *resumed == job)
        }),
        "the fatal tick must not resume a manual job with no surviving worker"
    );
    assert_eq!(
        state.survival().player().unwrap().vitality(),
        Vitality::ZERO
    );
    assert_eq!(state.player_work().active(), None);
    assert_eq!(
        state.production().get_job(job).unwrap().suspension(),
        suspension_before
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .unwrap()
            .reserved_inbound(),
        reserved_before
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        matter_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded: LoadedSaveEnvelope = serde_json::from_slice(&saved).unwrap();
    assert_eq!(loaded.into_state(&registries), Ok(state));
}

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
