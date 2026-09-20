//! Production completion ordering, provenance, stale-state, and committed-snapshot contracts.

use super::*;

#[test]
fn same_tick_completions_are_emitted_in_stable_job_id_order() {
    let registries = make_test_registries();
    let mut state = AppState::new(WorldSeed::new(14));
    let source = add_test_stockpile(&mut state, 100);
    let destination = add_test_stockpile(&mut state, 100);
    deposit_test_wood(&registries, &mut state, source, 20);

    let first_resolution = make_test_resolution(&registries, &mut state, source);
    let duration = first_resolution.duration();
    let first =
        match validate_start_process(&registries, &state, &first_resolution, source, destination) {
            Ok(token) => commit_process_for_test(token, &mut state),
            Err(error) => panic!("first process validation failed: {error}"),
        };
    let second_resolution = make_test_resolution(&registries, &mut state, source);
    let second = match validate_start_process(
        &registries,
        &state,
        &second_resolution,
        source,
        destination,
    ) {
        Ok(token) => commit_process_for_test(token, &mut state),
        Err(error) => panic!("second process validation failed: {error}"),
    };
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|record| record.reserved_inbound()),
        Some(Mass::from_milligrams(20)),
        "two same-tick jobs must reserve their shared destination cumulatively"
    );
    let inventory_revision_before_completion = state.inventory().revision();
    for _ in 1..duration.value() {
        let outcome = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("pre-completion tick failed: {error}"));
        assert!(outcome.production_completions().is_empty());
    }
    let outcome = match advance_tick(&registries, &mut state) {
        Ok(outcome) => outcome,
        Err(error) => panic!("completion tick failed: {error}"),
    };
    let completed: Vec<_> = outcome
        .production_completions()
        .iter()
        .map(|completion| completion.job())
        .collect();
    assert_eq!(completed, vec![first, second]);
    assert_eq!(
        state.inventory().revision(),
        inventory_revision_before_completion + 1,
        "one completion batch must apply all shared-destination deposits under one inventory revision"
    );
    let destination_record = state
        .inventory()
        .get_stockpile(destination)
        .unwrap_or_else(|| panic!("shared completion destination disappeared"));
    assert_eq!(destination_record.stored_mass(), Mass::from_milligrams(20));
    assert_eq!(destination_record.reserved_inbound(), Mass::ZERO);
    assert_eq!(state.inventory().lot_ids(destination).count(), 1);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn compatible_nonperishable_production_outputs_coalesce_and_preserve_provenance_range() {
    let registries = make_test_registries();
    let mut state = AppState::new(WorldSeed::new(141));
    let source = add_test_stockpile(&mut state, 100);
    let destination = add_test_stockpile(&mut state, 100);
    deposit_test_wood(&registries, &mut state, source, 20);

    let first_resolution = make_test_resolution(&registries, &mut state, source);
    let duration = first_resolution.duration();
    let first =
        match validate_start_process(&registries, &state, &first_resolution, source, destination) {
            Ok(token) => token,
            Err(error) => panic!("first process validation failed: {error}"),
        };
    commit_process_for_test(first, &mut state);
    let mut first_outcome = None;
    for _ in 0..duration.value() {
        first_outcome = Some(
            advance_tick(&registries, &mut state)
                .unwrap_or_else(|error| panic!("first completion failed: {error}")),
        );
    }
    let first_outcome =
        first_outcome.unwrap_or_else(|| panic!("first production resolution had zero duration"));
    let first_parcel = &first_outcome.production_completions()[0].landings()[0].parcels()[0];
    assert_eq!(first_parcel.output().mass(), Mass::from_milligrams(10));
    let surviving_lot = first_parcel.lot();

    let second_resolution = make_test_resolution(&registries, &mut state, source);
    let second = match validate_start_process(
        &registries,
        &state,
        &second_resolution,
        source,
        destination,
    ) {
        Ok(token) => token,
        Err(error) => panic!("second process validation failed: {error}"),
    };
    commit_process_for_test(second, &mut state);
    let mut second_outcome = None;
    for _ in 0..duration.value() {
        second_outcome = Some(
            advance_tick(&registries, &mut state)
                .unwrap_or_else(|error| panic!("second completion failed: {error}")),
        );
    }
    let second_outcome =
        second_outcome.unwrap_or_else(|| panic!("second production resolution had zero duration"));
    let second_parcel = &second_outcome.production_completions()[0].landings()[0].parcels()[0];
    assert_eq!(second_parcel.output().mass(), Mass::from_milligrams(10));
    assert_eq!(
        second_parcel.lot(),
        surviving_lot,
        "coalesced production receipt must retain the pre-existing surviving lot identity"
    );

    let lot_ids: Vec<_> = state.inventory().lot_ids(destination).collect();
    assert_eq!(lot_ids.len(), 1);
    assert_eq!(lot_ids[0], surviving_lot);
    let lot = state
        .inventory()
        .get_lot(lot_ids[0])
        .unwrap_or_else(|| panic!("coalesced production output disappeared"));
    assert_eq!(lot.mass(), Mass::from_milligrams(20));
    assert_eq!(lot.created_at(), SimulationTick::new(duration.value()));
    assert_eq!(
        lot.latest_created_at(),
        SimulationTick::new(duration.value() * 2)
    );
}

#[test]
fn resolution_source_mismatch_is_rejected_before_any_start_mutation() {
    let registries = make_test_registries();
    let mut state = AppState::new(WorldSeed::new(1415));
    let source = add_test_stockpile(&mut state, 100);
    let other_source = add_test_stockpile(&mut state, 100);
    let destination = add_test_stockpile(&mut state, 100);
    deposit_test_wood(&registries, &mut state, source, 10);
    deposit_test_wood(&registries, &mut state, other_source, 10);
    let resolution = make_test_resolution(&registries, &mut state, source);
    let before = state.clone();

    assert_eq!(
        validate_start_process(&registries, &state, &resolution, other_source, destination,),
        Err(StartProcessError::ResolutionSourceMismatch {
            bound: source,
            requested: other_source,
        })
    );
    assert_eq!(state, before);
}

#[test]
fn resolved_inputs_become_stale_after_inventory_changes_before_start_validation() {
    let registries = make_test_registries();
    let mut state = AppState::new(WorldSeed::new(1416));
    let source = add_test_stockpile(&mut state, 100);
    let destination = add_test_stockpile(&mut state, 100);
    deposit_test_wood(&registries, &mut state, source, 20);
    let resolution = make_test_resolution(&registries, &mut state, source);
    let expected_revision = state.inventory().revision();
    add_test_stockpile(&mut state, 1);
    let before = state.clone();

    assert_eq!(
        validate_start_process(&registries, &state, &resolution, source, destination),
        Err(StartProcessError::StaleResolvedInputs {
            expected_inventory_revision: expected_revision,
            actual_inventory_revision: expected_revision + 1,
        })
    );
    assert_eq!(state, before);
}

#[test]
fn stale_inventory_revision_rejects_validated_process_without_mutation() {
    let registries = make_test_registries();
    let mut state = AppState::new(WorldSeed::new(15));
    let source = add_test_stockpile(&mut state, 100);
    let destination = add_test_stockpile(&mut state, 100);
    deposit_test_wood(&registries, &mut state, source, 20);
    let resolution = make_test_resolution(&registries, &mut state, source);
    let token = match validate_start_process(&registries, &state, &resolution, source, destination)
    {
        Ok(token) => token,
        Err(error) => panic!("process validation failed: {error}"),
    };

    add_test_stockpile(&mut state, 1);
    let before_commit = state.clone();
    let result = token.commit(&mut state);

    assert!(matches!(
        result,
        Err(StartProcessCommitError::StaleInventoryRevision {
            expected: _expected,
            actual: _actual,
        })
    ));
    assert_eq!(state, before_commit);
}

#[test]
fn stale_production_revision_rejects_second_validated_token_without_mutation() {
    let registries = make_test_registries();
    let mut state = AppState::new(WorldSeed::new(16));
    let source = add_test_stockpile(&mut state, 100);
    let destination = add_test_stockpile(&mut state, 100);
    deposit_test_wood(&registries, &mut state, source, 30);
    let resolution = make_test_resolution(&registries, &mut state, source);
    let stale = match validate_start_process(&registries, &state, &resolution, source, destination)
    {
        Ok(token) => token,
        Err(error) => panic!("first process validation failed: {error}"),
    };
    let winner = match validate_start_process(&registries, &state, &resolution, source, destination)
    {
        Ok(token) => token,
        Err(error) => panic!("second process validation failed: {error}"),
    };
    commit_process_for_test(winner, &mut state);
    let before_stale_commit = state.clone();

    let result = stale.commit(&mut state);

    assert!(matches!(
        result,
        Err(StartProcessCommitError::StaleProductionRevision {
            expected: _expected,
            actual: _actual,
        })
    ));
    assert_eq!(state, before_stale_commit);
}

#[test]
fn in_flight_job_uses_committed_output_snapshot_after_later_resolution_differs() {
    let registries = make_test_registries();
    let mut state = AppState::new(WorldSeed::new(17));
    let source = add_test_stockpile(&mut state, 100);
    let destination = add_test_stockpile(&mut state, 100);
    deposit_test_wood(&registries, &mut state, source, 10);
    let resources = add_test_heating_resources(&registries, &mut state);
    let resolution = resolve_test_heating(
        &registries,
        &state,
        TEST_PROCESS,
        source,
        resources,
        TEST_TARGET_TEMPERATURE,
    );
    let later_resolution = resolve_test_heating(
        &registries,
        &state,
        TEST_PROCESS,
        source,
        resources,
        Temperature::from_millikelvin(1_000_000),
    );
    assert_ne!(resolution.outputs(), later_resolution.outputs());
    let duration = resolution.duration();
    let token = match validate_start_process(&registries, &state, &resolution, source, destination)
    {
        Ok(token) => token,
        Err(error) => panic!("original process validation failed: {error}"),
    };
    commit_process_for_test(token, &mut state);

    for _ in 0..duration.value() {
        if let Err(error) = advance_tick(&registries, &mut state) {
            panic!("completion after later resolution change failed: {error}");
        }
    }

    let destination_record = match state.inventory().get_stockpile(destination) {
        Some(record) => record,
        None => panic!("destination disappeared"),
    };
    assert_eq!(
        destination_record.get_mass(wood_log()),
        Mass::from_milligrams(10)
    );
    let lot_id = match state.inventory().lot_ids(destination).next() {
        Some(id) => id,
        None => panic!("committed output lot is missing"),
    };
    let lot = match state.inventory().get_lot(lot_id) {
        Some(lot) => lot,
        None => panic!("committed output lot record is missing"),
    };
    assert_eq!(lot.temperature(), TEST_TARGET_TEMPERATURE);
}
