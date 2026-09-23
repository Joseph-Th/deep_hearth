//! Structural topology, damage, embodiment, support, and derived-load persistence contracts.

use super::*;

#[test]
fn structural_graph_damage_and_load_round_trip_exactly() {
    let registries = build_registries();
    let mut state = AppState::new();
    let left = make_test_structural_element(&registries, &mut state, 0, 0, true);
    let right = make_test_structural_element(&registries, &mut state, 2, 0, true);
    let deck = make_test_structural_element(&registries, &mut state, 1, 0, false);
    activate_test_structural_element(&registries, &mut state, left);
    activate_test_structural_element(&registries, &mut state, right);
    link_test_structural_support(&registries, &mut state, deck, left);
    link_test_structural_support(&registries, &mut state, deck, right);
    activate_test_structural_element(&registries, &mut state, deck);
    let load = match validate_set_structural_load(
        &registries,
        &state,
        deck,
        StructuralLoadKind::Snow,
        Force::from_millinewtons(35_000_000),
    ) {
        Ok(token) => token,
        Err(error) => panic!("structural persistence load validation failed: {error}"),
    };
    let _ = commit_test_structural_mutation(load, &mut state);
    assert!(
        state
            .structures()
            .get_element(deck)
            .is_some_and(|record| record.is_cracked())
    );

    let encoded = match serde_json::to_vec(&SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("structural save serialization failed: {error}"),
    };
    let decoded: LoadedSaveEnvelope = match serde_json::from_slice(&encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("structural save deserialization failed: {error}"),
    };
    let loaded = match decoded.into_state(&registries) {
        Ok(loaded) => loaded,
        Err(error) => panic!("structural save validation failed: {error}"),
    };

    assert_eq!(loaded, state);
    let loaded_supports: Vec<_> = match loaded.structures().supports(deck) {
        Some(supports) => supports.collect(),
        None => panic!("loaded deck lost structural support index"),
    };
    assert_eq!(loaded_supports, vec![left, right]);
    assert_eq!(
        loaded
            .structures()
            .get_element(deck)
            .map(|record| record.load(StructuralLoadKind::Snow)),
        Some(Force::from_millinewtons(35_000_000))
    );
    let analysis = match analyze_structure(
        registries.structural(),
        registries.materials(),
        loaded.structures(),
    ) {
        Ok(analysis) => analysis,
        Err(error) => panic!("loaded structure analysis failed: {error}"),
    };
    assert!(analysis.damage_events().is_empty());
}

#[test]
fn obsolete_structural_embodied_mass_field_is_rejected_during_decode() {
    let registries = build_registries();
    let mut state = AppState::new();
    let member = make_test_structural_element(&registries, &mut state, 0, 0, true);
    let mut encoded = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("structural embodied-mass save serialization failed: {error}"),
    };
    encoded["state"]["systems"]["structures"]["elements"][member.value().to_string()]["embodied_mass"] =
        serde_json::json!(2_u64);
    assert!(serde_json::from_value::<LoadedSaveEnvelope>(encoded).is_err());
}

#[test]
fn tampered_structural_length_cannot_change_required_embodied_mass() {
    let registries = build_registries();
    let mut state = AppState::new();
    let member = make_test_structural_element(&registries, &mut state, 0, 0, true);
    let mut encoded = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("structural length save serialization failed: {error}"),
    };
    encoded["state"]["systems"]["structures"]["elements"][member.value().to_string()]["configuration"]
        ["geometry"]["length"] = serde_json::json!(2_u64);
    let decoded: LoadedSaveEnvelope = match serde_json::from_value(encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("tampered structural length save failed decode: {error}"),
    };

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Structure(
            StructureValidationError::EmbodiedMassGeometryMismatch {
                element: member,
                embodied: Mass::from_milligrams(1),
                required: Mass::from_milligrams(2),
            }
        )))
    );
}

#[test]
fn tampered_structural_self_weight_is_rejected_on_load() {
    let registries = build_registries();
    let mut state = AppState::new();
    let member = make_test_structural_element(&registries, &mut state, 0, 0, true);
    let mut encoded = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("structural self-weight save serialization failed: {error}"),
    };
    encoded["state"]["systems"]["structures"]["elements"][member.value().to_string()]["loads"]["SelfWeight"] =
        serde_json::json!(2_u128);
    let decoded: LoadedSaveEnvelope = match serde_json::from_value(encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("tampered structural self-weight save failed decode: {error}"),
    };

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Structure(
            StructureValidationError::SelfWeightMismatch {
                element: member,
                stored: Force::from_millinewtons(2),
                expected: Force::from_millinewtons(1),
            }
        )))
    );
}

#[test]
fn tampered_planned_structural_damage_is_rejected_on_load() {
    let registries = build_registries();
    let mut state = AppState::new();
    let member = make_test_structural_element(&registries, &mut state, 0, 0, true);
    let mut encoded = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("planned structural damage save serialization failed: {error}"),
    };
    encoded["state"]["systems"]["structures"]["elements"][member.value().to_string()]["is_cracked"] =
        serde_json::json!(true);
    let decoded: LoadedSaveEnvelope = match serde_json::from_value(encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("tampered planned structural damage save failed decode: {error}"),
    };

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Structure(
            StructureValidationError::PlannedElementCracked { element: member }
        )))
    );
}

#[test]
fn tampered_structural_cycle_is_rejected_on_load() {
    let registries = build_registries();
    let mut state = AppState::new();
    let first = make_test_structural_element(&registries, &mut state, 0, 0, false);
    let second = make_test_structural_element(&registries, &mut state, 1, 0, false);
    link_test_structural_support(&registries, &mut state, first, second);
    let mut encoded = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("structural cycle save serialization failed: {error}"),
    };
    encoded["state"]["systems"]["structures"]["supports_by_element"][second.value().to_string()] =
        serde_json::json!([first.value()]);
    let decoded: LoadedSaveEnvelope = match serde_json::from_value(encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("tampered structural cycle save failed decode: {error}"),
    };

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Structure(
            StructureValidationError::SupportCycle {
                element: first,
                support: second,
            }
        )))
    );
}

#[test]
fn tampered_structural_support_across_empty_space_is_rejected_on_load() {
    let registries = build_registries();
    let mut state = AppState::new();
    let member = make_test_structural_element(&registries, &mut state, 0, 0, false);
    let nearby_support = make_test_structural_element(&registries, &mut state, 1, 0, false);
    let distant_support = make_test_structural_element(&registries, &mut state, 4, 0, false);
    link_test_structural_support(&registries, &mut state, member, nearby_support);
    let mut encoded = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("structural contact save serialization failed: {error}"),
    };
    encoded["state"]["systems"]["structures"]["supports_by_element"][member.value().to_string()] =
        serde_json::json!([distant_support.value()]);
    let decoded: LoadedSaveEnvelope = match serde_json::from_value(encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("tampered structural contact save failed decode: {error}"),
    };

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Structure(
            StructureValidationError::SupportOutOfContact {
                element: member,
                support: distant_support,
            }
        )))
    );
}

#[test]
fn tampered_structural_support_with_only_edge_contact_is_rejected_on_load() {
    let registries = build_registries();
    let mut state = AppState::new();
    let member = make_test_structural_element(&registries, &mut state, 0, 0, false);
    let face_support = make_test_structural_element(&registries, &mut state, 1, 0, false);
    let edge_support = make_test_structural_element(&registries, &mut state, 1, 1, false);
    link_test_structural_support(&registries, &mut state, member, face_support);
    let mut encoded = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("structural edge-contact save serialization failed: {error}"),
    };
    encoded["state"]["systems"]["structures"]["supports_by_element"][member.value().to_string()] =
        serde_json::json!([edge_support.value()]);
    let decoded: LoadedSaveEnvelope = match serde_json::from_value(encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("tampered structural edge-contact save failed decode: {error}"),
    };

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Structure(
            StructureValidationError::SupportOutOfContact {
                element: member,
                support: edge_support,
            }
        )))
    );
}

#[test]
fn save_with_unresolved_structural_overload_is_rejected() {
    let registries = build_registries();
    let mut state = AppState::new();
    let column = make_test_structural_element(&registries, &mut state, 0, 0, true);
    activate_test_structural_element(&registries, &mut state, column);
    let mut encoded = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("structural overload save serialization failed: {error}"),
    };
    encoded["state"]["systems"]["structures"]["elements"][column.value().to_string()]["loads"]["Snow"] =
        serde_json::json!(50_000_000_u64);
    let decoded: LoadedSaveEnvelope = match serde_json::from_value(encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("tampered structural overload save failed decode: {error}"),
    };

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(
            StateValidationError::UnresolvedStructuralDamage {
                event: StructuralDamageEvent::Failed {
                    element: column,
                    cause: StructuralFailureCause::Overloaded {
                        carried_load: Force::from_millinewtons(50_000_001),
                        effective_capacity: Force::from_millinewtons(40_000_000),
                    },
                },
            }
        ))
    );
}

#[test]
fn current_save_rejects_registry_schema_mismatch() {
    let registries = build_registries();
    let state = AppState::new();
    let mut encoded = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("registry mismatch save serialization failed: {error}"),
    };
    let mismatched = RegistrySchemaVersion::new(u32::MAX);
    encoded["registry_schema_version"] = serde_json::json!(mismatched.value());
    let decoded: LoadedSaveEnvelope = match serde_json::from_value(encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("registry mismatch save failed decode: {error}"),
    };

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::RegistrySchemaMismatch {
            found: mismatched,
            supported: registries.schema_version(),
        })
    );
}

#[test]
fn supported_stockpile_round_trip_preserves_reverse_index_and_derived_load() {
    let registries = build_registries();
    let mut state = AppState::new();
    let support = make_test_structural_element(&registries, &mut state, 0, 0, true);
    activate_test_structural_element(&registries, &mut state, support);
    let stockpile = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000)) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("supported persistence stockpile failed: {error}"),
    };
    if let Err(error) = deposit_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(1_000),
        Temperature::from_millikelvin(293_150),
    ) {
        panic!("supported persistence material failed: {error}");
    }
    let mount = match validate_mount_stockpile(&registries, &state, stockpile, support) {
        Ok(mount) => mount,
        Err(error) => panic!("supported persistence mount failed: {error}"),
    };
    if let Err(error) = mount.commit(&mut state) {
        panic!("supported persistence mount commit failed: {error}");
    }
    let expected_load = state
        .structures()
        .get_element(support)
        .map(|record| record.load(StructuralLoadKind::StoredMatter));

    let encoded = match serde_json::to_vec(&SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("supported stockpile save serialization failed: {error}"),
    };
    let decoded: LoadedSaveEnvelope = match serde_json::from_slice(&encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("supported stockpile save decode failed: {error}"),
    };
    let loaded = match decoded.into_state(&registries) {
        Ok(loaded) => loaded,
        Err(error) => panic!("supported stockpile save validation failed: {error}"),
    };

    assert_eq!(loaded, state);
    assert_eq!(
        loaded
            .inventory()
            .get_stockpile(stockpile)
            .and_then(|record| record.supported_by()),
        Some(support)
    );
    assert_eq!(
        loaded
            .structures()
            .get_element(support)
            .map(|record| record.load(StructuralLoadKind::StoredMatter)),
        expected_load
    );
}

#[test]
fn tampered_stored_matter_load_is_rejected_on_load() {
    let registries = build_registries();
    let mut state = AppState::new();
    let support = make_test_structural_element(&registries, &mut state, 0, 0, true);
    activate_test_structural_element(&registries, &mut state, support);
    let stockpile = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000)) {
        Ok(stockpile) => stockpile,
        Err(error) => panic!("support corruption stockpile failed: {error}"),
    };
    if let Err(error) = deposit_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(1_000),
        Temperature::from_millikelvin(293_150),
    ) {
        panic!("support corruption material failed: {error}");
    }
    let mount = match validate_mount_stockpile(&registries, &state, stockpile, support) {
        Ok(mount) => mount,
        Err(error) => panic!("support corruption mount failed: {error}"),
    };
    if let Err(error) = mount.commit(&mut state) {
        panic!("support corruption mount commit failed: {error}");
    }

    let mut wrong_load = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("stored-matter tamper serialization failed: {error}"),
    };
    wrong_load["state"]["systems"]["structures"]["elements"][support.value().to_string()]["loads"]
        ["StoredMatter"] = serde_json::json!(999_u128);
    let wrong_load: LoadedSaveEnvelope = match serde_json::from_value(wrong_load) {
        Ok(decoded) => decoded,
        Err(error) => panic!("stored-matter tamper failed decode: {error}"),
    };
    assert_eq!(
        wrong_load.into_state(&registries),
        Err(LoadError::InvalidState(
            StateValidationError::StoredMatterStructuralLoadMismatch {
                element: support,
                stored: Force::from_millinewtons(999),
                expected: Force::from_millinewtons(10),
            }
        ))
    );
}
