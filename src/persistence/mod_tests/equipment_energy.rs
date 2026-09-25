//! Energy-store and equipment identity, support, capacity, and definition persistence contracts.

use super::*;

#[test]
fn energy_store_round_trip_preserves_definition_energy_and_revision() {
    let registries = make_test_energy_registries();
    let mut state = AppState::new();
    let store = match add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        TEST_ENERGY_DEFINITION,
        Energy::from_nanojoules(600_000),
    ) {
        Ok(store) => store,
        Err(error) => panic!("energy persistence fixture failed: {error}"),
    };

    let encoded = match serde_json::to_vec(&SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("energy save serialization failed: {error}"),
    };
    let decoded: LoadedSaveEnvelope = match serde_json::from_slice(&encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("energy save deserialization failed: {error}"),
    };
    let loaded = match decoded.into_state(&registries) {
        Ok(loaded) => loaded,
        Err(error) => panic!("energy save validation failed: {error}"),
    };

    let record = match loaded.energy().get_store(store) {
        Some(record) => record,
        None => panic!("energy store disappeared after round trip"),
    };
    assert_eq!(record.definition(), TEST_ENERGY_DEFINITION);
    assert_eq!(record.stored(), Energy::from_nanojoules(600_000));
    assert_eq!(loaded.energy().revision(), 1);
    assert_eq!(loaded, state);
}

#[test]
fn deserialized_zero_energy_store_id_is_rejected_on_load() {
    let registries = make_test_energy_registries();
    let mut state = AppState::new();
    let store = add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        TEST_ENERGY_DEFINITION,
        Energy::from_nanojoules(100),
    )
    .unwrap_or_else(|error| panic!("zero-id energy fixture failed: {error}"));
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("zero-id energy save serialization failed: {error}"));
    let records = encoded["state"]["systems"]["energy"]["records"]
        .as_object_mut()
        .unwrap_or_else(|| panic!("energy records fixture is not an object"));
    let mut record = records
        .remove(&store.value().to_string())
        .unwrap_or_else(|| panic!("energy fixture record disappeared before tampering"));
    record["id"] = serde_json::json!(0_u64);
    records.insert("0".to_owned(), record);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("zero-id energy save failed decode: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Energy(
            EnergyValidationError::ZeroStoreId
        )))
    );
}

#[test]
fn mounted_equipment_round_trip_preserves_support_and_derived_structural_load() {
    let registries = make_test_equipment_registries();
    let mut state = AppState::new();
    let support = make_test_structural_element(&registries, &mut state, 0, 0, true);
    activate_test_structural_element(&registries, &mut state, support);
    let equipment = match add_equipment(
        &registries,
        &mut state,
        TEST_EQUIPMENT_DEFINITION,
        condition(575_000),
    ) {
        Ok(equipment) => equipment,
        Err(error) => panic!("mounted equipment fixture creation failed: {error}"),
    };
    let mount = match validate_mount_equipment(&registries, &state, equipment, support) {
        Ok(mount) => mount,
        Err(error) => panic!("mounted equipment support validation failed: {error}"),
    };
    if let Err(error) = mount.commit(&mut state) {
        panic!("mounted equipment support commit failed: {error}");
    }
    let expected_load = state
        .structures()
        .get_element(support)
        .map(|record| record.load(StructuralLoadKind::Equipment));

    let encoded = match serde_json::to_vec(&SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("mounted equipment save serialization failed: {error}"),
    };
    let decoded: LoadedSaveEnvelope = match serde_json::from_slice(&encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("mounted equipment save deserialization failed: {error}"),
    };
    let loaded = match decoded.into_state(&registries) {
        Ok(loaded) => loaded,
        Err(error) => panic!("mounted equipment save validation failed: {error}"),
    };

    assert_eq!(
        loaded
            .equipment()
            .get_equipment(equipment)
            .and_then(|record| record.supported_by()),
        Some(support)
    );
    assert_eq!(
        loaded
            .structures()
            .get_element(support)
            .map(|record| record.load(StructuralLoadKind::Equipment)),
        expected_load
    );
    assert_eq!(loaded, state);
}

#[test]
fn mounted_equipment_with_missing_support_is_rejected_on_load() {
    let registries = make_test_equipment_registries();
    let mut state = AppState::new();
    let support = make_test_structural_element(&registries, &mut state, 0, 0, true);
    activate_test_structural_element(&registries, &mut state, support);
    let equipment = match add_equipment(
        &registries,
        &mut state,
        TEST_EQUIPMENT_DEFINITION,
        Condition::PRISTINE,
    ) {
        Ok(equipment) => equipment,
        Err(error) => panic!("missing-support equipment fixture failed: {error}"),
    };
    let mount = match validate_mount_equipment(&registries, &state, equipment, support) {
        Ok(mount) => mount,
        Err(error) => panic!("missing-support mount validation failed: {error}"),
    };
    if let Err(error) = mount.commit(&mut state) {
        panic!("missing-support mount commit failed: {error}");
    }
    let missing = StructuralElementId::new(999_991);
    let mut encoded = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("missing-support save serialization failed: {error}"),
    };
    encoded["state"]["systems"]["equipment"]["records"][equipment.value().to_string()]["supported_by"] =
        serde_json::json!(missing.value());
    let decoded: LoadedSaveEnvelope = match serde_json::from_value(encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("missing-support tampered save failed decode: {error}"),
    };

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(
            StateValidationError::UnknownEquipmentSupport {
                equipment,
                element: missing,
            }
        ))
    );
}

#[test]
fn tampered_equipment_structural_load_is_rejected_on_load() {
    let registries = make_test_equipment_registries();
    let mut state = AppState::new();
    let support = make_test_structural_element(&registries, &mut state, 0, 0, true);
    activate_test_structural_element(&registries, &mut state, support);
    let equipment = match add_equipment(
        &registries,
        &mut state,
        TEST_EQUIPMENT_DEFINITION,
        Condition::PRISTINE,
    ) {
        Ok(equipment) => equipment,
        Err(error) => panic!("load-tamper equipment fixture failed: {error}"),
    };
    let mount = match validate_mount_equipment(&registries, &state, equipment, support) {
        Ok(mount) => mount,
        Err(error) => panic!("load-tamper mount validation failed: {error}"),
    };
    if let Err(error) = mount.commit(&mut state) {
        panic!("load-tamper mount commit failed: {error}");
    }
    let expected = match state.structures().get_element(support) {
        Some(record) => record.load(StructuralLoadKind::Equipment),
        None => panic!("load-tamper support disappeared"),
    };
    let stored = Force::from_millinewtons(expected.millinewtons() - 1);
    let mut encoded = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("load-tamper save serialization failed: {error}"),
    };
    encoded["state"]["systems"]["structures"]["elements"][support.value().to_string()]["loads"]["Equipment"] =
        serde_json::json!(stored.millinewtons());
    let decoded: LoadedSaveEnvelope = match serde_json::from_value(encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("load-tamper save failed decode: {error}"),
    };

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(
            StateValidationError::EquipmentStructuralLoadMismatch {
                element: support,
                stored,
                expected,
            }
        ))
    );
}

#[test]
fn energy_store_with_unknown_definition_is_rejected_on_load() {
    let registries = make_test_energy_registries();
    let mut state = AppState::new();
    let store = match add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        TEST_ENERGY_DEFINITION,
        Energy::from_nanojoules(100),
    ) {
        Ok(store) => store,
        Err(error) => panic!("energy persistence fixture failed: {error}"),
    };
    let unknown = EnergyStoreDefinitionId::new(999_992);
    let mut encoded = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("energy save serialization failed: {error}"),
    };
    encoded["state"]["systems"]["energy"]["records"][store.value().to_string()]["definition"] =
        serde_json::json!(unknown.value());
    let decoded: LoadedSaveEnvelope = match serde_json::from_value(encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("tampered energy save failed decode: {error}"),
    };

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Energy(
            EnergyValidationError::UnknownDefinition {
                store,
                definition: unknown,
            }
        )))
    );
}

#[test]
fn energy_store_above_authored_capacity_is_rejected_on_load() {
    let registries = make_test_energy_registries();
    let mut state = AppState::new();
    let store = match add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        TEST_ENERGY_DEFINITION,
        Energy::from_nanojoules(100),
    ) {
        Ok(store) => store,
        Err(error) => panic!("energy persistence fixture failed: {error}"),
    };
    let mut encoded = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("energy save serialization failed: {error}"),
    };
    encoded["state"]["systems"]["energy"]["records"][store.value().to_string()]["stored"] =
        serde_json::json!(1_000_001_u64);
    let decoded: LoadedSaveEnvelope = match serde_json::from_value(encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("tampered energy capacity save failed decode: {error}"),
    };

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Energy(
            EnergyValidationError::CapacityExceeded {
                store,
                stored: Energy::from_nanojoules(1_000_001),
                capacity: Energy::from_nanojoules(1_000_000),
            }
        )))
    );
}

#[test]
fn equipment_round_trip_preserves_definition_condition_and_revision() {
    let registries = make_test_equipment_registries();
    let mut state = AppState::new();
    let equipment = match add_equipment(
        &registries,
        &mut state,
        TEST_EQUIPMENT_DEFINITION,
        condition(575_000),
    ) {
        Ok(equipment) => equipment,
        Err(error) => panic!("equipment fixture creation failed: {error}"),
    };

    let encoded = match serde_json::to_vec(&SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("equipment save serialization failed: {error}"),
    };
    let decoded: LoadedSaveEnvelope = match serde_json::from_slice(&encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("equipment save deserialization failed: {error}"),
    };
    let loaded = match decoded.into_state(&registries) {
        Ok(loaded) => loaded,
        Err(error) => panic!("equipment save validation failed: {error}"),
    };

    let loaded_equipment = match loaded.equipment().get_equipment(equipment) {
        Some(record) => record,
        None => panic!("equipment disappeared after round trip"),
    };
    assert_eq!(loaded_equipment.definition(), TEST_EQUIPMENT_DEFINITION);
    assert_eq!(loaded_equipment.condition(), condition(575_000));
    assert_eq!(loaded.equipment().revision(), 1);
    assert_eq!(loaded, state);
}

#[test]
fn equipment_with_unknown_definition_is_rejected_on_load() {
    let registries = make_test_equipment_registries();
    let mut state = AppState::new();
    let equipment = match add_equipment(
        &registries,
        &mut state,
        TEST_EQUIPMENT_DEFINITION,
        Condition::PRISTINE,
    ) {
        Ok(equipment) => equipment,
        Err(error) => panic!("equipment fixture creation failed: {error}"),
    };
    let unknown_definition = EquipmentDefinitionId::new(999_991);
    let mut encoded = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("equipment save serialization failed: {error}"),
    };
    encoded["state"]["systems"]["equipment"]["records"][equipment.value().to_string()]["definition"] =
        serde_json::json!(unknown_definition.value());
    let decoded: LoadedSaveEnvelope = match serde_json::from_value(encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("tampered equipment save failed structural decode: {error}"),
    };

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Equipment(
            EquipmentValidationError::UnknownDefinition {
                equipment,
                definition: unknown_definition,
            }
        )))
    );
}
