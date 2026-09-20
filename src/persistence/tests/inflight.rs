//! In-flight sensible-heating energy-trace and deterministic continuation contracts.

use super::*;

#[test]
fn in_flight_sensible_heating_round_trip_preserves_energy_trace_and_continuation() {
    let registries = make_test_heating_registries();
    let mut state = AppState::new(WorldSeed::new(0xE900_0100));
    let source = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
        Ok(id) => id,
        Err(error) => panic!("heating persistence source failed: {error}"),
    };
    let destination = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
        Ok(id) => id,
        Err(error) => panic!("heating persistence destination failed: {error}"),
    };
    let input_lot = match deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(300_000),
    ) {
        Ok(lot) => lot,
        Err(error) => panic!("heating persistence material fixture failed: {error}"),
    };
    let equipment = match add_equipment(
        &registries,
        &mut state,
        TEST_HEATER_DEFINITION,
        Condition::PRISTINE,
    ) {
        Ok(id) => id,
        Err(error) => panic!("heating persistence equipment failed: {error}"),
    };
    let energy_store = match add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        TEST_HEAT_ENERGY_DEFINITION,
        Energy::from_nanojoules(500_000_000),
    ) {
        Ok(id) => id,
        Err(error) => panic!("heating persistence energy failed: {error}"),
    };
    let resolved = match resolve_sensible_heating_process(
        &registries,
        &state,
        SensibleHeatingRequest::new(
            TEST_HEAT_PROCESS,
            source,
            &[MaterialLotSelection::new(
                input_lot,
                Mass::from_milligrams(10),
            )],
            equipment,
            energy_store,
            Temperature::from_millikelvin(303_000),
        ),
    ) {
        Ok(resolved) => resolved,
        Err(error) => panic!("heating persistence resolution failed: {error}"),
    };
    let duration = resolved.process_resolution().duration();
    let expected_energy = resolved.process_resolution().energy_input();
    let expected_equipment = resolved.process_resolution().equipment_input();
    let expected_equipment_condition_after =
        resolved.process_resolution().equipment_condition_after();
    let token = match validate_start_process(
        &registries,
        &state,
        resolved.process_resolution(),
        source,
        destination,
    ) {
        Ok(token) => token,
        Err(error) => panic!("heating persistence start validation failed: {error}"),
    };
    let job = match token.commit(&mut state) {
        Ok(job) => job,
        Err(error) => panic!("heating persistence start commit failed: {error}"),
    };
    assert_eq!(
        state
            .production()
            .get_job(job)
            .and_then(|record| record.consumed_energy()),
        expected_energy
    );
    assert_eq!(
        state
            .production()
            .get_job(job)
            .and_then(|record| record.equipment_provider()),
        expected_equipment
    );
    assert_eq!(
        state
            .production()
            .get_job(job)
            .and_then(|record| record.equipment_condition_after()),
        expected_equipment_condition_after
    );

    let mut tampered = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("heating provenance tamper serialization failed: {error}"),
    };
    tampered["state"]["systems"]["production"]["jobs"][job.value().to_string()]["resources"]["consumed_energy"]
        ["carrier"] = serde_json::json!("Thermal");
    let tampered: LoadedSaveEnvelope = match serde_json::from_value(tampered) {
        Ok(decoded) => decoded,
        Err(error) => panic!("heating provenance tamper failed decode: {error}"),
    };
    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(
            StateValidationError::JobEnergyCarrierMismatch {
                job,
                traced: EnergyCarrier::Thermal,
                authored: EnergyCarrier::Electrical,
            }
        ))
    );

    let mut tampered_condition_outcome =
        match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
            Ok(encoded) => encoded,
            Err(error) => panic!("heating wear tamper serialization failed: {error}"),
        };
    tampered_condition_outcome["state"]["systems"]["production"]["jobs"][job.value().to_string()]
        ["equipment"]["condition_after"] = serde_json::json!(999_999_u32);
    let tampered_condition_outcome: LoadedSaveEnvelope =
        match serde_json::from_value(tampered_condition_outcome) {
            Ok(decoded) => decoded,
            Err(error) => panic!("heating wear tamper failed decode: {error}"),
        };
    assert_eq!(
        tampered_condition_outcome.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::ThermalJob(
            ThermalJobValidationError::EquipmentConditionOutcomeMismatch {
                job,
                stored: condition(999_999),
                required: condition(999_000),
            }
        )))
    );

    let mut tampered_energy = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("heating energy tamper serialization failed: {error}"),
    };
    tampered_energy["state"]["systems"]["production"]["jobs"][job.value().to_string()]["resources"]
        ["consumed_energy"]["energy"] = serde_json::json!(1_u64);
    let tampered_energy: LoadedSaveEnvelope = match serde_json::from_value(tampered_energy) {
        Ok(decoded) => decoded,
        Err(error) => panic!("heating energy tamper failed decode: {error}"),
    };
    assert_eq!(
        tampered_energy.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::ThermalJob(
            ThermalJobValidationError::EnergyMismatch {
                job,
                traced: Energy::from_nanojoules(1),
                required: Energy::from_nanojoules(51_000_000),
            }
        )))
    );

    let mut tampered_duration = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("heating duration tamper serialization failed: {error}"),
    };
    tampered_duration["state"]["systems"]["production"]["jobs"][job.value().to_string()]["schedule"]
        ["active_duration"] = serde_json::json!(duration.value() + 1);
    let tampered_duration: LoadedSaveEnvelope = match serde_json::from_value(tampered_duration) {
        Ok(decoded) => decoded,
        Err(error) => panic!("heating duration tamper failed decode: {error}"),
    };
    assert_eq!(
        tampered_duration.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::ThermalJob(
            ThermalJobValidationError::DurationMismatch {
                job,
                stored: crate::core::time::TickSpan::new(duration.value() + 1),
                required: duration,
            }
        )))
    );

    let mut tampered_output = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("heating output tamper serialization failed: {error}"),
    };
    tampered_output["state"]["systems"]["production"]["jobs"][job.value().to_string()]["output_streams"]
        [0]["outputs"][0]["commodity"] =
        serde_json::json!(CommodityKey::new(MATERIAL_WOOD, FORM_LUMP).value());
    let tampered_output: LoadedSaveEnvelope = match serde_json::from_value(tampered_output) {
        Ok(decoded) => decoded,
        Err(error) => panic!("heating output tamper failed decode: {error}"),
    };
    assert_eq!(
        tampered_output.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::ThermalJob(
            ThermalJobValidationError::OutputMismatch { job }
        )))
    );

    let mut tampered_equipment = match serde_json::to_value(SaveEnvelope::new(&registries, &state))
    {
        Ok(encoded) => encoded,
        Err(error) => panic!("heating equipment tamper serialization failed: {error}"),
    };
    tampered_equipment["state"]["systems"]["production"]["jobs"][job.value().to_string()]["equipment"]
        ["provider"]["condition"] = serde_json::json!(999_999_u32);
    let tampered_equipment: LoadedSaveEnvelope = match serde_json::from_value(tampered_equipment) {
        Ok(decoded) => decoded,
        Err(error) => panic!("heating equipment tamper failed decode: {error}"),
    };
    assert_eq!(
        tampered_equipment.into_state(&registries),
        Err(LoadError::InvalidState(
            StateValidationError::JobEquipmentConditionMismatch {
                job,
                traced: condition(999_999),
                stored: Condition::PRISTINE,
            }
        ))
    );

    let mut double_booked = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("heating double-book tamper serialization failed: {error}"),
    };
    let second_job = job.value() + 1;
    let mut duplicated =
        double_booked["state"]["systems"]["production"]["jobs"][job.value().to_string()].clone();
    duplicated["identity"]["id"] = serde_json::json!(second_job);
    double_booked["state"]["systems"]["production"]["jobs"][second_job.to_string()] = duplicated;
    double_booked["state"]["systems"]["production"]["next_job_id"] =
        serde_json::json!(second_job + 1);
    let double_booked: LoadedSaveEnvelope = match serde_json::from_value(double_booked) {
        Ok(decoded) => decoded,
        Err(error) => panic!("heating double-book tamper failed decode: {error}"),
    };
    assert_eq!(
        double_booked.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Production(
            ProductionValidationError::EnergyDoubleBooked {
                store: energy_store
            }
        )))
    );

    let mut equipment_double_booked =
        match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
            Ok(encoded) => encoded,
            Err(error) => panic!("equipment double-book tamper serialization failed: {error}"),
        };
    let second_equipment_job = job.value() + 1;
    let mut duplicated_equipment =
        equipment_double_booked["state"]["systems"]["production"]["jobs"][job.value().to_string()]
            .clone();
    duplicated_equipment["identity"]["id"] = serde_json::json!(second_equipment_job);
    duplicated_equipment["resources"]["consumed_energy"] = serde_json::Value::Null;
    equipment_double_booked["state"]["systems"]["production"]["jobs"]
        [second_equipment_job.to_string()] = duplicated_equipment;
    equipment_double_booked["state"]["systems"]["production"]["next_job_id"] =
        serde_json::json!(second_equipment_job + 1);
    let equipment_double_booked: LoadedSaveEnvelope =
        match serde_json::from_value(equipment_double_booked) {
            Ok(decoded) => decoded,
            Err(error) => panic!("equipment double-book tamper failed decode: {error}"),
        };
    assert_eq!(
        equipment_double_booked.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Production(
            ProductionValidationError::EquipmentDoubleBooked { equipment }
        )))
    );

    let encoded = match serde_json::to_vec(&SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("heating in-flight save serialization failed: {error}"),
    };
    let decoded: LoadedSaveEnvelope = match serde_json::from_slice(&encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("heating in-flight save deserialization failed: {error}"),
    };
    let mut resumed = match decoded.into_state(&registries) {
        Ok(state) => state,
        Err(error) => panic!("heating in-flight save validation failed: {error}"),
    };
    let mut uninterrupted = state.clone();
    assert_eq!(resumed, uninterrupted);
    assert_eq!(
        resumed
            .production()
            .get_job(job)
            .and_then(|record| record.consumed_energy()),
        expected_energy
    );
    assert_eq!(
        resumed
            .production()
            .get_job(job)
            .and_then(|record| record.equipment_provider()),
        expected_equipment
    );
    assert_eq!(
        resumed
            .production()
            .get_job(job)
            .and_then(|record| record.equipment_condition_after()),
        expected_equipment_condition_after
    );

    for _ in 0..duration.value() {
        let uninterrupted_outcome = match advance_tick(&registries, &mut uninterrupted) {
            Ok(outcome) => outcome,
            Err(error) => panic!("uninterrupted heating continuation failed: {error}"),
        };
        let resumed_outcome = match advance_tick(&registries, &mut resumed) {
            Ok(outcome) => outcome,
            Err(error) => panic!("resumed heating continuation failed: {error}"),
        };
        assert_eq!(resumed_outcome, uninterrupted_outcome);
    }
    assert_eq!(resumed, uninterrupted);
    assert!(resumed.production().get_job(job).is_none());
    let output = match resumed
        .inventory()
        .lots()
        .find(|lot| lot.stockpile() == destination)
    {
        Some(lot) => lot,
        None => panic!("resumed heating output disappeared"),
    };
    assert_eq!(output.temperature(), Temperature::from_millikelvin(303_000));
    assert_eq!(output.mass(), Mass::from_milligrams(10));
    assert_eq!(
        resumed
            .equipment()
            .get_equipment(equipment)
            .map(|record| record.condition()),
        expected_equipment_condition_after
    );
}
