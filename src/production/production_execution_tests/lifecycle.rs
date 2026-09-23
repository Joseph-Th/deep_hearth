//! Production custody, timing, storage-history, and trusted-load contracts.

use super::*;

#[test]
fn process_consumes_inputs_reserves_capacity_and_completes_on_due_tick() {
    let registries = make_test_registries();
    let mut state = AppState::new();
    let source = add_test_stockpile(&mut state, 100);
    let destination = add_test_stockpile(&mut state, 100);
    deposit_test_wood(&registries, &mut state, source, 20);
    let resolution = make_test_resolution(&registries, &mut state, source);
    let duration = resolution.duration();

    let token = match validate_start_process(&registries, &state, &resolution, source, destination)
    {
        Ok(token) => token,
        Err(error) => panic!("process validation failed: {error}"),
    };
    let job = commit_process_for_test(token, &mut state);

    let source_record = match state.inventory().get_stockpile(source) {
        Some(record) => record,
        None => panic!("source disappeared"),
    };
    let destination_record = match state.inventory().get_stockpile(destination) {
        Some(record) => record,
        None => panic!("destination disappeared"),
    };
    assert_eq!(
        source_record.get_mass(wood_log()),
        Mass::from_milligrams(10)
    );
    assert_eq!(
        destination_record.reserved_inbound(),
        Mass::from_milligrams(10)
    );
    assert_eq!(
        state
            .production()
            .get_job(job)
            .map(ProductionJobRecord::completes_at),
        Some(SimulationTick::new(duration.value()))
    );

    for expected_tick in 1..duration.value() {
        let outcome = match advance_tick(&registries, &mut state) {
            Ok(outcome) => outcome,
            Err(error) => panic!("tick failed: {error}"),
        };
        assert_eq!(outcome.tick(), SimulationTick::new(expected_tick));
        assert!(outcome.production_completions().is_empty());
    }

    let outcome = match advance_tick(&registries, &mut state) {
        Ok(outcome) => outcome,
        Err(error) => panic!("completion tick failed: {error}"),
    };
    assert_eq!(outcome.production_completions().len(), 1);
    assert_eq!(outcome.production_completions()[0].job(), job);
    assert!(state.production().get_job(job).is_none());
    let destination_record = match state.inventory().get_stockpile(destination) {
        Some(record) => record,
        None => panic!("destination disappeared"),
    };
    assert_eq!(destination_record.reserved_inbound(), Mass::ZERO);
    assert_eq!(
        destination_record.get_mass(wood_log()),
        Mass::from_milligrams(10)
    );
    let output_lots: Vec<_> = state.inventory().lot_ids(destination).collect();
    assert_eq!(output_lots.len(), 1);
    let output_lot = match state.inventory().get_lot(output_lots[0]) {
        Some(lot) => lot,
        None => panic!("completed output lot disappeared"),
    };
    assert_eq!(output_lot.temperature(), TEST_TARGET_TEMPERATURE);
    assert_eq!(
        output_lot.created_at(),
        SimulationTick::new(duration.value())
    );
}

#[test]
fn production_preserves_input_storage_exposure_and_ages_work_in_process() {
    let registries = make_test_registries_with_standard_sensible_heating(TEST_PERISHABLE_PROCESS);
    let mut state = AppState::new();
    let preserved_profile = StockpileStorageProfile::with_preservation(
        true,
        false,
        Temperature::from_millikelvin(350_000),
        3_000_000,
    )
    .unwrap_or_else(|error| panic!("perishable source profile failed: {error}"));
    let source = add_stockpile(&mut state, Mass::from_milligrams(20), preserved_profile)
        .unwrap_or_else(|error| panic!("perishable source stockpile failed: {error}"));
    let destination = add_test_stockpile(&mut state, 20);
    let input_lot = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        berry_food(),
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("perishable input deposit failed: {error}"));

    apply_clock_advance(&mut state, SimulationTick::new(6));
    assert_eq!(
        assess_food_freshness(&registries, &state, input_lot),
        Ok(FoodFreshness::Fresh {
            age: TickSpan::new(2),
            remaining: TickSpan::new(287_994),
        })
    );

    let resolution = make_resolution_for_process(
        &registries,
        &mut state,
        source,
        TEST_PERISHABLE_PROCESS,
        Temperature::from_millikelvin(500_000),
    );
    let duration = resolution.duration();
    let token = validate_start_process(&registries, &state, &resolution, source, destination)
        .unwrap_or_else(|error| panic!("perishable process start failed: {error}"));
    commit_process_for_test(token, &mut state);

    for _ in 0..duration.value() {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("perishable process tick failed: {error}"));
    }

    let output_lot = state
        .inventory()
        .lot_ids(destination)
        .next()
        .unwrap_or_else(|| panic!("perishable process output lot disappeared"));
    let output = state
        .inventory()
        .get_lot(output_lot)
        .unwrap_or_else(|| panic!("perishable process output record disappeared"));
    assert_eq!(
        output.created_at(),
        SimulationTick::new(6 + duration.value())
    );
    assert!(matches!(
        assess_food_freshness(&registries, &state, output_lot),
        Ok(FoodFreshness::Fresh { age, .. })
            if age == TickSpan::new(2 + duration.value())
    ));
}

#[test]
fn persisted_production_storage_history_must_be_rebased_to_job_start() {
    let registries = make_test_registries();
    let mut state = AppState::new();
    let source = add_test_stockpile(&mut state, 100);
    let destination = add_test_stockpile(&mut state, 100);
    deposit_test_wood(&registries, &mut state, source, 10);
    let resolution = make_test_resolution(&registries, &mut state, source);
    let token = validate_start_process(&registries, &state, &resolution, source, destination)
        .unwrap_or_else(|error| panic!("storage-history validation fixture failed: {error}"));
    let job = commit_process_for_test(token, &mut state);
    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("storage-history validation tick failed: {error}"));

    let encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("storage-history fixture serialization failed: {error}"));
    let mut transition_tampered = encoded.clone();
    transition_tampered["state"]["systems"]["production"]["jobs"][job.value().to_string()]["resources"]
        ["material_storage_history"]["last_transition_at"] = serde_json::json!(1_u64);
    let tampered: LoadedSaveEnvelope = serde_json::from_value(transition_tampered)
        .unwrap_or_else(|error| panic!("storage-history tamper failed structural decode: {error}"));

    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Production(
            ProductionValidationError::StorageHistoryTransitionMismatch {
                job,
                transition: SimulationTick::new(1),
                started_at: SimulationTick::new(0),
            }
        )))
    );

    let mut age_tampered = encoded;
    age_tampered["state"]["systems"]["production"]["jobs"][job.value().to_string()]["resources"]
        ["material_storage_history"]["ambient_age_parts"] = serde_json::json!(u64::MAX);
    let serialized = serde_json::to_string(&age_tampered)
        .unwrap_or_else(|error| panic!("storage-age tamper serialization failed: {error}"));
    let sentinel = format!("\"ambient_age_parts\":{}", u64::MAX);
    let overflow = format!("\"ambient_age_parts\":{}", u128::MAX);
    assert_eq!(serialized.matches(&sentinel).count(), 1);
    let overflowed = serialized.replacen(&sentinel, &overflow, 1);
    assert!(serde_json::from_str::<LoadedSaveEnvelope>(&overflowed).is_err());
}

#[test]
fn production_started_after_time_elapsed_rebases_storage_history_to_ownership_boundary() {
    let registries = make_test_registries();
    let mut state = AppState::new();
    let source = add_test_stockpile(&mut state, 100);
    let destination = add_test_stockpile(&mut state, 100);
    deposit_test_wood(&registries, &mut state, source, 10);
    apply_clock_advance(&mut state, SimulationTick::new(5));
    let resolution = make_test_resolution(&registries, &mut state, source);
    let token = validate_start_process(&registries, &state, &resolution, source, destination)
        .unwrap_or_else(|error| panic!("later production start failed: {error}"));
    let job = commit_process_for_test(token, &mut state);
    let record = state
        .production()
        .get_job(job)
        .unwrap_or_else(|| panic!("later-start production job disappeared"));

    assert_eq!(record.started_at(), SimulationTick::new(5));
    assert_eq!(
        record.material_storage_history().last_transition_at(),
        record.started_at(),
        "production custody must checkpoint storage exposure exactly at its ownership boundary"
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn persisted_production_job_cannot_start_in_the_future() {
    let registries = make_test_registries();
    let mut state = AppState::new();
    let source = add_test_stockpile(&mut state, 100);
    let destination = add_test_stockpile(&mut state, 100);
    deposit_test_wood(&registries, &mut state, source, 10);
    let resolution = make_test_resolution(&registries, &mut state, source);
    let token = validate_start_process(&registries, &state, &resolution, source, destination)
        .unwrap_or_else(|error| panic!("future-start validation fixture failed: {error}"));
    let job = commit_process_for_test(token, &mut state);

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("future-start fixture serialization failed: {error}"));
    encoded["state"]["systems"]["production"]["jobs"][job.value().to_string()]["schedule"]["started_at"] =
        serde_json::json!(1_u64);
    encoded["state"]["systems"]["production"]["jobs"][job.value().to_string()]["resources"]["material_storage_history"]
        ["last_transition_at"] = serde_json::json!(1_u64);
    let tampered: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("future-start tamper failed structural decode: {error}"));

    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Production(
            ProductionValidationError::JobStartedInFuture {
                job,
                started_at: SimulationTick::new(1),
                current: SimulationTick::ZERO,
            }
        )))
    );
}

#[test]
fn persisted_running_production_job_cannot_already_be_due() {
    let registries = make_test_registries();
    let mut state = AppState::new();
    let source = add_test_stockpile(&mut state, 100);
    let destination = add_test_stockpile(&mut state, 100);
    deposit_test_wood(&registries, &mut state, source, 10);
    let resolution = make_test_resolution(&registries, &mut state, source);
    let token = validate_start_process(&registries, &state, &resolution, source, destination)
        .unwrap_or_else(|error| panic!("already-due validation fixture failed: {error}"));
    let job = commit_process_for_test(token, &mut state);
    apply_clock_advance(&mut state, SimulationTick::new(1));

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("already-due fixture serialization failed: {error}"));
    encoded["state"]["systems"]["production"]["jobs"][job.value().to_string()]["schedule"]["completes_at"] =
        serde_json::json!(1_u64);
    let tampered: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("already-due tamper failed structural decode: {error}"));

    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Production(
            ProductionValidationError::RunningJobAlreadyDue {
                job,
                due: SimulationTick::new(1),
                current: SimulationTick::new(1),
            }
        )))
    );
}

#[test]
fn persisted_running_production_job_cannot_complete_before_active_duration() {
    let registries = make_test_registries();
    let mut state = AppState::new();
    let source = add_test_stockpile(&mut state, 100);
    let destination = add_test_stockpile(&mut state, 100);
    deposit_test_wood(&registries, &mut state, source, 10);
    let resolution = make_test_resolution(&registries, &mut state, source);
    let duration = resolution.duration();
    let token = validate_start_process(&registries, &state, &resolution, source, destination)
        .unwrap_or_else(|error| panic!("early-due validation fixture failed: {error}"));
    let job = commit_process_for_test(token, &mut state);
    apply_clock_advance(&mut state, SimulationTick::new(1));

    let expected_due = SimulationTick::new(duration.value());
    let forged_due = SimulationTick::new(
        duration
            .value()
            .checked_sub(1)
            .unwrap_or_else(|| panic!("heating fixture duration must exceed zero")),
    );
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("early-due fixture serialization failed: {error}"));
    encoded["state"]["systems"]["production"]["jobs"][job.value().to_string()]["schedule"]["completes_at"] =
        serde_json::json!(forged_due.value());
    let tampered: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("early-due tamper failed structural decode: {error}"));

    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Production(
            ProductionValidationError::CompletionScheduleMismatch {
                job,
                expected_due,
                actual_due: forged_due,
            }
        )))
    );
}

#[test]
fn persisted_heating_job_rejects_consumed_state_hotter_than_committed_target() {
    let input = CommodityKey::new(MATERIAL_COPPER, FORM_INGOT);
    let registries = make_test_registries_with_standard_sensible_heating(TEST_COMPOSITION_PROCESS);
    let mut state = AppState::new();
    let source = add_test_stockpile(&mut state, 20);
    let destination = add_test_stockpile(&mut state, 20);
    deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        input,
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(300_000),
    )
    .unwrap_or_else(|error| panic!("phase-validation input fixture failed: {error}"));
    let resolution = make_resolution_for_process(
        &registries,
        &mut state,
        source,
        TEST_COMPOSITION_PROCESS,
        TEST_TARGET_TEMPERATURE,
    );
    let token = validate_start_process(&registries, &state, &resolution, source, destination)
        .unwrap_or_else(|error| panic!("phase-validation process start failed: {error}"));
    let job = commit_process_for_test(token, &mut state);
    let melting_point = registries
        .materials()
        .get_material(MATERIAL_COPPER)
        .and_then(|definition| definition.properties().thermal().melting_point())
        .unwrap_or_else(|| panic!("copper fixture lost its authored melting point"));
    let invalid_temperature = Temperature::from_millikelvin(
        melting_point
            .millikelvin()
            .checked_add(1)
            .unwrap_or_else(|| panic!("copper melting point cannot exhaust temperature range")),
    );

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("phase-validation fixture serialization failed: {error}"));
    encoded["state"]["systems"]["production"]["jobs"][job.value().to_string()]["resources"]["consumed_inputs"]
        [0]["profile"]["temperature"] = serde_json::json!(invalid_temperature.millikelvin());
    let tampered: LoadedSaveEnvelope = serde_json::from_value(encoded).unwrap_or_else(|error| {
        panic!("phase-validation tamper failed structural decode: {error}")
    });

    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::ThermalJob(
            ThermalJobValidationError::TargetBelowInputTemperature {
                job,
                current: invalid_temperature,
                target: TEST_TARGET_TEMPERATURE,
            }
        )))
    );
}
