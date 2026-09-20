//! Production output routing, capacity, conservation, and start atomicity contracts.

use super::*;

#[test]
fn routed_output_streams_reserve_and_complete_by_identity_not_route_order() {
    let registries = make_test_registries_with_standard_screening(TEST_PROCESS);
    let mut state = AppState::new(WorldSeed::new(10_001));
    let source = add_test_stockpile(&mut state, 20);
    let undersize_destination = add_test_stockpile(&mut state, 10);
    let oversize_destination = add_test_stockpile(&mut state, 10);
    let resolved = make_test_multi_stream_resolution(&registries, &mut state, source);
    let resolution = resolved.process_resolution();
    let duration = resolution.duration();
    assert_eq!(
        resolution
            .output_streams()
            .iter()
            .map(|stream| stream.id())
            .collect::<Vec<_>>(),
        vec![
            ScreeningProcessDefinition::UNDERSIZE_STREAM,
            ScreeningProcessDefinition::OVERSIZE_STREAM,
        ]
    );

    let token = match validate_start_process_routed(
        &registries,
        &state,
        resolution,
        source,
        &[
            ProcessOutputRoute::new(
                ScreeningProcessDefinition::OVERSIZE_STREAM,
                oversize_destination,
            ),
            ProcessOutputRoute::new(
                ScreeningProcessDefinition::UNDERSIZE_STREAM,
                undersize_destination,
            ),
        ],
    ) {
        Ok(token) => token,
        Err(error) => panic!("multi-stream process validation failed: {error}"),
    };
    let job = commit_process_for_test(token, &mut state);

    assert_eq!(
        state
            .inventory()
            .get_stockpile(undersize_destination)
            .map(|record| record.reserved_inbound()),
        Some(Mass::from_milligrams(6))
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(undersize_destination)
            .map(|record| record.available_capacity()),
        Some(Mass::from_milligrams(4)),
        "available capacity must include committed production output"
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(oversize_destination)
            .map(|record| record.reserved_inbound()),
        Some(Mass::from_milligrams(4))
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(oversize_destination)
            .map(|record| record.available_capacity()),
        Some(Mass::from_milligrams(6)),
        "available capacity must distinguish each destination's reservation"
    );
    let stored_routes = match state.production().get_job(job) {
        Some(record) => record
            .output_streams()
            .iter()
            .map(|stream| (stream.id(), stream.destination()))
            .collect::<Vec<_>>(),
        None => panic!("multi-stream job disappeared after start"),
    };
    assert_eq!(
        stored_routes,
        vec![
            (
                ScreeningProcessDefinition::UNDERSIZE_STREAM,
                undersize_destination,
            ),
            (
                ScreeningProcessDefinition::OVERSIZE_STREAM,
                oversize_destination,
            ),
        ]
    );
    if let Err(error) = validate_loaded_state(&registries, &state) {
        panic!("multi-stream running state failed validation: {error}");
    }
    let encoded = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("multi-stream save serialization failed: {error}"),
    };
    let mut noncanonical = encoded.clone();
    let streams = match noncanonical["state"]["systems"]["production"]["jobs"]
        [job.value().to_string()]["output_streams"]
        .as_array_mut()
    {
        Some(streams) => streams,
        None => panic!("multi-stream save omitted production output streams"),
    };
    streams.reverse();
    let noncanonical: LoadedSaveEnvelope = match serde_json::from_value(noncanonical) {
        Ok(loaded) => loaded,
        Err(error) => {
            panic!("noncanonical multi-stream save failed structural decode: {error}")
        }
    };
    assert_eq!(
        noncanonical.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Production(
            ProductionValidationError::NonCanonicalOutputStreamOrder { job }
        )))
    );

    let mut duplicate_stream = encoded.clone();
    let streams = duplicate_stream["state"]["systems"]["production"]["jobs"]
        [job.value().to_string()]["output_streams"]
        .as_array_mut()
        .unwrap_or_else(|| panic!("multi-stream duplicate test lost output streams"));
    streams.push(streams[0].clone());
    let duplicate_stream: LoadedSaveEnvelope = serde_json::from_value(duplicate_stream)
        .unwrap_or_else(|error| {
            panic!("duplicate output-stream save failed structural decode: {error}")
        });
    assert_eq!(
        duplicate_stream.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Production(
            ProductionValidationError::DuplicateOutputStreamId {
                job,
                stream: ScreeningProcessDefinition::UNDERSIZE_STREAM,
            }
        )))
    );

    let mut duplicate_output = encoded.clone();
    let outputs = duplicate_output["state"]["systems"]["production"]["jobs"]
        [job.value().to_string()]["output_streams"][0]["outputs"]
        .as_array_mut()
        .unwrap_or_else(|| panic!("multi-stream duplicate test lost stream outputs"));
    outputs.push(outputs[0].clone());
    let duplicate_output: LoadedSaveEnvelope = serde_json::from_value(duplicate_output)
        .unwrap_or_else(|error| {
            panic!("duplicate output-spec save failed structural decode: {error}")
        });
    assert_eq!(
        duplicate_output.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Production(
            ProductionValidationError::DuplicateOutputSpecification { job }
        )))
    );

    let loaded: LoadedSaveEnvelope = match serde_json::from_value(encoded) {
        Ok(loaded) => loaded,
        Err(error) => panic!("multi-stream save deserialization failed: {error}"),
    };
    let restored = match loaded.into_state(&registries) {
        Ok(restored) => restored,
        Err(error) => panic!("multi-stream save validation failed: {error}"),
    };
    assert_eq!(restored, state);

    for _ in 1..duration.value() {
        let outcome = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("multi-stream pre-completion tick failed: {error}"));
        assert!(outcome.production_completions().is_empty());
    }
    let outcome = match advance_tick(&registries, &mut state) {
        Ok(outcome) => outcome,
        Err(error) => panic!("multi-stream completion tick failed: {error}"),
    };
    assert_eq!(outcome.production_completions().len(), 1);
    assert_eq!(outcome.production_completions()[0].job(), job);
    assert_eq!(
        outcome.production_completions()[0].routes(),
        [
            ProcessOutputRoute::new(
                ScreeningProcessDefinition::UNDERSIZE_STREAM,
                undersize_destination,
            ),
            ProcessOutputRoute::new(
                ScreeningProcessDefinition::OVERSIZE_STREAM,
                oversize_destination,
            ),
        ]
    );
    let completion = &outcome.production_completions()[0];
    assert_eq!(completion.landings().len(), 2);
    assert_eq!(
        completion.landings()[0].stream(),
        ScreeningProcessDefinition::UNDERSIZE_STREAM
    );
    assert_eq!(
        completion.landings()[0].destination(),
        undersize_destination
    );
    assert_eq!(completion.landings()[0].parcels().len(), 1);
    assert_eq!(
        completion.landings()[1].stream(),
        ScreeningProcessDefinition::OVERSIZE_STREAM
    );
    assert_eq!(completion.landings()[1].destination(), oversize_destination);
    assert_eq!(completion.landings()[1].parcels().len(), 1);
    let undersize_parcel = &completion.landings()[0].parcels()[0];
    assert_eq!(
        undersize_parcel.output().commodity(),
        CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED)
    );
    assert_eq!(undersize_parcel.output().mass(), Mass::from_milligrams(6));
    let undersize_landing = state
        .inventory()
        .get_lot(undersize_parcel.lot())
        .unwrap_or_else(|| panic!("undersize completion landing disappeared"));
    assert_eq!(undersize_landing.stockpile(), undersize_destination);
    let oversize_parcel = &completion.landings()[1].parcels()[0];
    assert_eq!(
        oversize_parcel.output().commodity(),
        CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED)
    );
    assert_eq!(oversize_parcel.output().mass(), Mass::from_milligrams(4));
    let oversize_landing = state
        .inventory()
        .get_lot(oversize_parcel.lot())
        .unwrap_or_else(|| panic!("oversize completion landing disappeared"));
    assert_eq!(oversize_landing.stockpile(), oversize_destination);
    let undersize_record = match state.inventory().get_stockpile(undersize_destination) {
        Some(record) => record,
        None => panic!("undersize destination disappeared"),
    };
    assert_eq!(undersize_record.reserved_inbound(), Mass::ZERO);
    assert_eq!(
        undersize_record.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED)),
        Mass::from_milligrams(6)
    );
    assert_eq!(
        undersize_record.available_capacity(),
        Mass::from_milligrams(4),
        "completion must replace reserved capacity with stored matter without changing free capacity"
    );
    let oversize_record = match state.inventory().get_stockpile(oversize_destination) {
        Some(record) => record,
        None => panic!("oversize destination disappeared"),
    };
    assert_eq!(oversize_record.reserved_inbound(), Mass::ZERO);
    assert_eq!(
        oversize_record.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED)),
        Mass::from_milligrams(4)
    );
    assert_eq!(
        oversize_record.available_capacity(),
        Mass::from_milligrams(6)
    );
}

#[test]
fn duplicate_output_route_is_rejected_atomically() {
    let registries = make_test_registries_with_standard_screening(TEST_PROCESS);
    let mut state = AppState::new(WorldSeed::new(10_002));
    let source = add_test_stockpile(&mut state, 20);
    let first_destination = add_test_stockpile(&mut state, 10);
    let second_destination = add_test_stockpile(&mut state, 10);
    let resolved = make_test_multi_stream_resolution(&registries, &mut state, source);
    let resolution = resolved.process_resolution();
    let before = state.clone();

    let result = validate_start_process_routed(
        &registries,
        &state,
        resolution,
        source,
        &[
            ProcessOutputRoute::new(
                ScreeningProcessDefinition::UNDERSIZE_STREAM,
                first_destination,
            ),
            ProcessOutputRoute::new(
                ScreeningProcessDefinition::UNDERSIZE_STREAM,
                second_destination,
            ),
        ],
    );

    assert_eq!(
        result,
        Err(StartProcessError::DuplicateOutputRoute {
            stream: ScreeningProcessDefinition::UNDERSIZE_STREAM,
        })
    );
    assert_eq!(state, before);
}

#[test]
fn shared_destination_capacity_is_checked_against_aggregate_stream_mass() {
    let registries = make_test_registries_with_standard_screening(TEST_PROCESS);
    let mut state = AppState::new(WorldSeed::new(10_003));
    let source = add_test_stockpile(&mut state, 20);
    let destination = add_test_stockpile(&mut state, 9);
    let resolved = make_test_multi_stream_resolution(&registries, &mut state, source);
    let resolution = resolved.process_resolution();
    let before = state.clone();

    let result = validate_start_process_routed(
        &registries,
        &state,
        resolution,
        source,
        &[
            ProcessOutputRoute::new(ScreeningProcessDefinition::UNDERSIZE_STREAM, destination),
            ProcessOutputRoute::new(ScreeningProcessDefinition::OVERSIZE_STREAM, destination),
        ],
    );

    assert_eq!(
        result,
        Err(StartProcessError::CapacityExceeded {
            stockpile: destination,
            capacity: Mass::from_milligrams(9),
            committed_after_consumption: Mass::ZERO,
            requested_inbound: Mass::from_milligrams(10),
        })
    );
    assert_eq!(state, before);
}

#[test]
fn failed_process_start_is_atomic() {
    let registries = make_test_registries();
    let mut state = AppState::new(WorldSeed::new(11));
    let source = add_test_stockpile(&mut state, 100);
    add_test_stockpile(&mut state, 100);
    deposit_test_wood(&registries, &mut state, source, 5);
    let before = state.clone();

    let lot = state
        .inventory()
        .lot_ids(source)
        .next()
        .unwrap_or_else(|| panic!("atomicity fixture lost its material lot"));
    let result = validate_process_inputs(
        &registries,
        &state,
        TEST_PROCESS,
        source,
        &[MaterialLotSelection::new(lot, Mass::from_milligrams(10))],
    );

    assert!(matches!(
        result,
        Err(ProcessInputError::InsufficientSelectedLotMass {
            lot: _lot,
            available: _available,
            requested: _requested,
        })
    ));
    assert_eq!(state, before);
}

#[test]
fn process_start_rejects_resolution_that_bypasses_registered_resource_topology() {
    let registries = make_test_registries();
    let mut state = AppState::new(WorldSeed::new(0x9000_0014));
    let source = add_test_stockpile(&mut state, 100);
    let destination = add_test_stockpile(&mut state, 100);
    deposit_test_wood(&registries, &mut state, source, 10);
    let inputs = bind_source_mass(
        &registries,
        &state,
        TEST_PROCESS,
        source,
        Mass::from_milligrams(10),
    );
    let resolution = inputs
        .resolve_without_resources(
            TickSpan::new(1),
            vec![MaterialLotSpec::new(
                wood_log(),
                Mass::from_milligrams(10),
                Temperature::from_millikelvin(500_000),
            )],
        )
        .unwrap_or_else(|error| panic!("topology-bypass fixture resolution failed: {error}"));
    let before = state.clone();

    assert_eq!(
        validate_start_process(&registries, &state, &resolution, source, destination),
        Err(StartProcessError::ResolutionEnergyTopologyMismatch {
            process: TEST_PROCESS,
        })
    );
    assert_eq!(state, before);
}

#[test]
fn resolved_process_cannot_create_or_destroy_unaccounted_matter() {
    let registries = make_test_registries();
    let mut state = AppState::new(WorldSeed::new(111));
    let source = add_test_stockpile(&mut state, 100);
    deposit_test_wood(&registries, &mut state, source, 10);
    let inputs = bind_source_mass(
        &registries,
        &state,
        TEST_PROCESS,
        source,
        Mass::from_milligrams(10),
    );
    let before = state.clone();

    let result = inputs.resolve_without_resources(
        TickSpan::new(3),
        vec![MaterialLotSpec::new(
            wood_log(),
            Mass::from_milligrams(9),
            Temperature::from_millikelvin(600_000),
        )],
    );

    assert!(matches!(
        result,
        Err(ProcessResolutionError::MatterBalanceMismatch {
            input_mass,
            output_mass,
        }) if input_mass == Mass::from_milligrams(10)
            && output_mass == Mass::from_milligrams(9)
    ));
    assert_eq!(state, before);
}

#[test]
fn reserved_output_capacity_cannot_be_taken_by_later_deposits() {
    let registries = make_test_registries();
    let mut state = AppState::new(WorldSeed::new(12));
    let source = add_test_stockpile(&mut state, 100);
    let destination = add_test_stockpile(&mut state, 12);
    deposit_test_wood(&registries, &mut state, source, 10);
    let resolution = make_test_resolution(&registries, &mut state, source);
    let token = match validate_start_process(&registries, &state, &resolution, source, destination)
    {
        Ok(token) => token,
        Err(error) => panic!("process validation failed: {error}"),
    };
    commit_process_for_test(token, &mut state);

    let result = deposit_bulk_for_test(
        &registries,
        &mut state,
        destination,
        wood_log(),
        Mass::from_milligrams(3),
    );

    assert!(matches!(
        result,
        Err(crate::inventory::MaterialFixtureError::Ingress(
            crate::inventory::MaterialIngressError::CapacityExceeded {
                stockpile: _stockpile,
                capacity: _capacity,
                committed: _committed,
                requested: _requested,
            }
        ))
    ));
}

#[test]
fn same_stockpile_process_accounts_for_consumed_space_before_reserving_output() {
    let registries = make_test_registries();
    let mut state = AppState::new(WorldSeed::new(13));
    let stockpile = add_test_stockpile(&mut state, 10);
    deposit_test_wood(&registries, &mut state, stockpile, 10);
    let resolution = make_test_resolution(&registries, &mut state, stockpile);

    let token = match validate_start_process(&registries, &state, &resolution, stockpile, stockpile)
    {
        Ok(token) => token,
        Err(error) => panic!("same-stockpile process validation failed: {error}"),
    };
    commit_process_for_test(token, &mut state);

    let record = match state.inventory().get_stockpile(stockpile) {
        Some(record) => record,
        None => panic!("stockpile disappeared"),
    };
    assert_eq!(record.stored_mass(), Mass::ZERO);
    assert_eq!(record.reserved_inbound(), Mass::from_milligrams(10));
}
