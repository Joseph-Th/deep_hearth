//! Production identity and owner-revision headroom contracts.

use super::*;

#[test]
fn process_start_rejects_exhausted_job_id_without_consuming_material() {
    let (registries, state, source, destination) = unstarted_process_fixture();
    let mut encoded =
        serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("production job-id exhaustion serialization failed: {error}")
        });
    encoded["state"]["systems"]["production"]["next_job_id"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("production job-id exhaustion decode failed: {error}"));
    let mut loaded = decoded.into_state(&registries).unwrap_or_else(|error| {
        panic!("production job-id exhaustion fixture should load: {error}")
    });
    let resolution = make_test_resolution(&registries, &mut loaded, source);
    let before = loaded.clone();

    assert_eq!(
        validate_start_process(&registries, &loaded, &resolution, source, destination).err(),
        Some(StartProcessError::JobIdExhausted)
    );
    assert_eq!(loaded, before);
}

#[test]
fn process_start_reserves_future_material_lot_identity_capacity() {
    let (registries, state, source, destination) = unstarted_process_fixture();
    let mut encoded =
        serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("production lot-id exhaustion serialization failed: {error}")
        });
    encoded["state"]["systems"]["inventory"]["next_lot_id"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("production lot-id exhaustion decode failed: {error}"));
    let mut loaded = decoded.into_state(&registries).unwrap_or_else(|error| {
        panic!("idle exhausted lot-id fixture should load before production admission: {error}")
    });
    let resolution = make_test_resolution(&registries, &mut loaded, source);
    let before = loaded.clone();

    assert_eq!(
        validate_start_process(&registries, &loaded, &resolution, source, destination).err(),
        Some(StartProcessError::MaterialLotIdExhausted)
    );
    assert_eq!(loaded, before);
}

#[test]
fn trusted_load_rejects_running_production_without_future_material_lot_identity_capacity() {
    let (registries, mut state, source, destination) = unstarted_process_fixture();
    let resolution = make_test_resolution(&registries, &mut state, source);
    let token = validate_start_process(&registries, &state, &resolution, source, destination)
        .unwrap_or_else(|error| panic!("production lot-budget validation failed: {error}"));
    let _ = commit_process_for_test(token, &mut state);

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("production lot-budget serialization failed: {error}"));
    encoded["state"]["systems"]["inventory"]["next_lot_id"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("production lot-budget decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(
            StateValidationError::FutureMaterialLotIdCapacityExhausted {
                next_lot_id: u64::MAX,
                required: 1,
            }
        ))
    );
}

#[test]
fn later_inventory_ingress_cannot_consume_identity_reserved_for_running_production() {
    let registries = make_test_registries();
    let mut state = AppState::new();
    let source = add_test_stockpile(&mut state, 100);
    let destination = add_test_stockpile(&mut state, 100);
    let unrelated_destination = add_test_stockpile(&mut state, 100);
    deposit_test_wood(&registries, &mut state, source, 20);
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("production lot-reservation serialization failed: {error}"));
    encoded["state"]["systems"]["inventory"]["next_lot_id"] = serde_json::json!(u64::MAX - 1);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("production lot-reservation decode failed: {error}"));
    let mut loaded = decoded.into_state(&registries).unwrap_or_else(|error| {
        panic!("near-exhausted lot-id fixture should load before production admission: {error}")
    });
    let resolution = make_test_resolution(&registries, &mut loaded, source);
    let token = validate_start_process(&registries, &loaded, &resolution, source, destination)
        .unwrap_or_else(|error| panic!("near-exhausted production admission failed: {error}"));
    let _ = commit_process_for_test(token, &mut loaded);
    let before = loaded.clone();

    assert_eq!(
        deposit_lot_for_test(
            &registries,
            &mut loaded,
            unrelated_destination,
            wood_log(),
            Mass::from_milligrams(1),
            Temperature::from_millikelvin(293_150),
        ),
        Err(MaterialFixtureError::Ingress(
            MaterialIngressError::LotIdExhausted
        ))
    );
    assert_eq!(loaded, before);
}

#[test]
fn production_completion_can_consume_the_last_reserved_material_lot_identity() {
    let (registries, state, source, destination) = unstarted_process_fixture();
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("last lot-id serialization failed: {error}"));
    encoded["state"]["systems"]["inventory"]["next_lot_id"] = serde_json::json!(u64::MAX - 1);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("last lot-id decode failed: {error}"));
    let mut loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("last lot-id fixture should load: {error}"));
    let resolution = make_test_resolution(&registries, &mut loaded, source);
    let token = validate_start_process(&registries, &loaded, &resolution, source, destination)
        .unwrap_or_else(|error| panic!("last lot-id production admission failed: {error}"));
    let job = commit_process_for_test(token, &mut loaded);

    while loaded.production().get_job(job).is_some() {
        let _ = advance_tick(&registries, &mut loaded)
            .unwrap_or_else(|error| panic!("last lot-id production completion failed: {error}"));
    }

    assert_eq!(loaded.inventory().next_lot_id(), u64::MAX);
    assert_eq!(loaded.checked_future_material_lot_id_demand(), Some(0));
    assert_eq!(loaded.inventory().lot_ids(destination).count(), 1);
    validate_loaded_state(&registries, &loaded)
        .unwrap_or_else(|error| panic!("last lot-id completed state failed validation: {error}"));
}

#[test]
fn process_start_rejects_exhausted_production_revision_without_consuming_material() {
    let (registries, state, source, destination) = unstarted_process_fixture();
    let mut encoded =
        serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("production revision exhaustion serialization failed: {error}")
        });
    encoded["state"]["systems"]["production"]["revision"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("production revision exhaustion decode failed: {error}"));
    let mut loaded = decoded.into_state(&registries).unwrap_or_else(|error| {
        panic!("production revision exhaustion fixture should load: {error}")
    });
    let resolution = make_test_resolution(&registries, &mut loaded, source);
    let before = loaded.clone();

    assert_eq!(
        validate_start_process(&registries, &loaded, &resolution, source, destination).err(),
        Some(StartProcessError::ProductionRevisionExhausted)
    );
    assert_eq!(loaded, before);
}

#[test]
fn process_start_reserves_revision_capacity_for_admission_and_completion() {
    let (registries, state, source, destination) = unstarted_process_fixture();
    let encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("production revision-budget serialization failed: {error}"));

    for (owner, expected) in [
        ("production", StartProcessError::ProductionRevisionExhausted),
        ("inventory", StartProcessError::InventoryRevisionExhausted),
    ] {
        let mut candidate = encoded.clone();
        candidate["state"]["systems"][owner]["revision"] = serde_json::json!(u64::MAX - 1);
        let decoded: LoadedSaveEnvelope = serde_json::from_value(candidate)
            .unwrap_or_else(|error| panic!("production revision-budget decode failed: {error}"));
        let mut loaded = decoded.into_state(&registries).unwrap_or_else(|error| {
            panic!("idle near-exhausted production owner should load: {error}")
        });
        let resolution = make_test_resolution(&registries, &mut loaded, source);
        let before = loaded.clone();

        assert_eq!(
            validate_start_process(&registries, &loaded, &resolution, source, destination).err(),
            Some(expected)
        );
        assert_eq!(loaded, before);
    }
}

#[test]
fn trusted_load_rejects_running_production_without_scheduled_completion_revision_capacity() {
    let (registries, mut state, source, destination) = unstarted_process_fixture();
    let resolution = make_test_resolution(&registries, &mut state, source);
    let token = validate_start_process(&registries, &state, &resolution, source, destination)
        .unwrap_or_else(|error| panic!("production load-budget validation failed: {error}"));
    let _ = commit_process_for_test(token, &mut state);

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("production load-budget serialization failed: {error}"));
    encoded["state"]["systems"]["production"]["revision"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("production load-budget decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Production(
            ProductionValidationError::ScheduledRevisionCapacityExhausted {
                revision: u64::MAX,
                completion_buckets: 1,
            }
        )))
    );
}

#[test]
fn trusted_load_rejects_running_production_without_inventory_completion_revision_capacity() {
    let (registries, mut state, source, destination) = unstarted_process_fixture();
    let resolution = make_test_resolution(&registries, &mut state, source);
    let token = validate_start_process(&registries, &state, &resolution, source, destination)
        .unwrap_or_else(|error| {
            panic!("production inventory load-budget validation failed: {error}")
        });
    let _ = commit_process_for_test(token, &mut state);

    let mut encoded =
        serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("production inventory load-budget serialization failed: {error}")
        });
    encoded["state"]["systems"]["inventory"]["revision"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("production inventory load-budget decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(
            StateValidationError::ProductionInventoryRevisionCapacityExhausted {
                revision: u64::MAX,
                completion_buckets: 1,
            }
        ))
    );
}

#[test]
fn process_start_preserves_inventory_revisions_owed_to_existing_due_buckets() {
    let registries = make_test_registries();
    let mut state = AppState::new();
    let first_source = add_test_stockpile(&mut state, 100);
    let first_destination = add_test_stockpile(&mut state, 100);
    let second_source = add_test_stockpile(&mut state, 100);
    let second_destination = add_test_stockpile(&mut state, 100);
    deposit_test_wood(&registries, &mut state, first_source, 20);
    deposit_test_wood(&registries, &mut state, second_source, 20);

    let first_resolution = make_test_resolution(&registries, &mut state, first_source);
    let first = validate_start_process(
        &registries,
        &state,
        &first_resolution,
        first_source,
        first_destination,
    )
    .unwrap_or_else(|error| panic!("existing production validation failed: {error}"));
    let first_job = commit_process_for_test(first, &mut state);
    let first_due = state
        .production()
        .get_job(first_job)
        .map(ProductionJobRecord::completes_at)
        .unwrap_or_else(|| panic!("existing production job disappeared"));
    let _ = advance_tick(&registries, &mut state).unwrap_or_else(|error| {
        panic!("existing production pre-second-start tick failed: {error}")
    });
    assert!(
        state.tick() < first_due,
        "fixture requires the first production job to remain running after one tick"
    );

    let mut encoded =
        serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("production inventory headroom serialization failed: {error}")
        });
    encoded["state"]["systems"]["inventory"]["revision"] = serde_json::json!(u64::MAX - 2);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("production inventory headroom decode failed: {error}"));
    let mut loaded = decoded.into_state(&registries).unwrap_or_else(|error| {
        panic!("one scheduled completion must fit at inventory revision MAX-2: {error}")
    });
    let second_resolution = make_test_resolution(&registries, &mut loaded, second_source);
    let before = loaded.clone();

    assert_eq!(
        validate_start_process(
            &registries,
            &loaded,
            &second_resolution,
            second_source,
            second_destination,
        )
        .err(),
        Some(StartProcessError::InventoryRevisionExhausted)
    );
    assert_eq!(loaded, before);
}

#[test]
fn unrelated_inventory_transfer_cannot_spend_revision_owed_to_running_production() {
    let (registries, mut state, source, destination) = unstarted_process_fixture();
    let unrelated_source = add_test_stockpile(&mut state, 100);
    let unrelated_destination = add_test_stockpile(&mut state, 100);
    deposit_test_wood(&registries, &mut state, unrelated_source, 10);
    let resolution = make_test_resolution(&registries, &mut state, source);
    let token = validate_start_process(&registries, &state, &resolution, source, destination)
        .unwrap_or_else(|error| panic!("revision-theft production validation failed: {error}"));
    let _ = commit_process_for_test(token, &mut state);

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("revision-theft serialization failed: {error}"));
    encoded["state"]["systems"]["inventory"]["revision"] = serde_json::json!(u64::MAX - 1);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("revision-theft decode failed: {error}"));
    let loaded = decoded.into_state(&registries).unwrap_or_else(|error| {
        panic!("one production completion must fit at inventory revision MAX-1: {error}")
    });
    let before = loaded.clone();

    assert_eq!(
        validate_material_transfer_for_test(
            &registries,
            &loaded,
            unrelated_source,
            unrelated_destination,
            wood_log(),
            Mass::from_milligrams(1),
        )
        .err(),
        Some(MaterialTransferError::RevisionExhausted)
    );
    assert_eq!(loaded, before);
}

#[test]
fn trusted_load_rejects_running_production_without_equipment_completion_revision_capacity() {
    let (registries, mut state, source, destination) = unstarted_process_fixture();
    let resolution = make_test_resolution(&registries, &mut state, source);
    let token = validate_start_process(&registries, &state, &resolution, source, destination)
        .unwrap_or_else(|error| {
            panic!("production equipment load-budget validation failed: {error}")
        });
    let _ = commit_process_for_test(token, &mut state);

    let mut encoded =
        serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("production equipment load-budget serialization failed: {error}")
        });
    encoded["state"]["systems"]["equipment"]["revision"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("production equipment load-budget decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(
            StateValidationError::ProductionEquipmentRevisionCapacityExhausted {
                revision: u64::MAX,
                completion_buckets: 1,
            }
        ))
    );
}

#[test]
fn process_start_preserves_equipment_revisions_owed_to_existing_wear_buckets() {
    let registries = make_test_registries();
    let mut state = AppState::new();
    let first_source = add_test_stockpile(&mut state, 100);
    let first_destination = add_test_stockpile(&mut state, 100);
    let second_source = add_test_stockpile(&mut state, 100);
    let second_destination = add_test_stockpile(&mut state, 100);
    deposit_test_wood(&registries, &mut state, first_source, 20);
    deposit_test_wood(&registries, &mut state, second_source, 20);
    let first_resources = add_test_heating_resources(&registries, &mut state);
    let second_resources = add_test_heating_resources(&registries, &mut state);

    let first_resolution = resolve_test_heating(
        &registries,
        &state,
        TEST_PROCESS,
        first_source,
        first_resources,
        TEST_TARGET_TEMPERATURE,
    );
    let first = validate_start_process(
        &registries,
        &state,
        &first_resolution,
        first_source,
        first_destination,
    )
    .unwrap_or_else(|error| panic!("existing wear production validation failed: {error}"));
    let first_job = commit_process_for_test(first, &mut state);
    let first_due = state
        .production()
        .get_job(first_job)
        .map(ProductionJobRecord::completes_at)
        .unwrap_or_else(|| panic!("existing wear production job disappeared"));
    let _ = advance_tick(&registries, &mut state).unwrap_or_else(|error| {
        panic!("existing wear production pre-second-start tick failed: {error}")
    });
    assert!(
        state.tick() < first_due,
        "fixture requires the first wear-bearing production job to remain running after one tick"
    );

    let mut encoded =
        serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("production equipment headroom serialization failed: {error}")
        });
    encoded["state"]["systems"]["equipment"]["revision"] = serde_json::json!(u64::MAX - 1);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("production equipment headroom decode failed: {error}"));
    let loaded = decoded.into_state(&registries).unwrap_or_else(|error| {
        panic!("one scheduled wear completion must fit at equipment revision MAX-1: {error}")
    });
    let second_resolution = resolve_test_heating(
        &registries,
        &loaded,
        TEST_PROCESS,
        second_source,
        second_resources,
        TEST_TARGET_TEMPERATURE,
    );
    let before = loaded.clone();

    assert_eq!(
        validate_start_process(
            &registries,
            &loaded,
            &second_resolution,
            second_source,
            second_destination,
        )
        .err(),
        Some(StartProcessError::EquipmentRevisionExhausted)
    );
    assert_eq!(loaded, before);
}

#[test]
fn process_start_reserves_completion_structure_revision_for_supported_output() {
    let (registries, mut state, source, destination) = unstarted_process_fixture();
    let support = add_active_stockpile_support(&registries, &mut state, 0);
    let _ = validate_mount_stockpile(&registries, &state, destination, support)
        .unwrap_or_else(|error| panic!("production destination mount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("production destination mount commit failed: {error}"));

    let mut encoded =
        serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("production structure-budget serialization failed: {error}")
        });
    encoded["state"]["systems"]["structures"]["revision"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("production structure-budget decode failed: {error}"));
    let mut loaded = decoded.into_state(&registries).unwrap_or_else(|error| {
        panic!("idle exhausted production structure owner should load: {error}")
    });
    let resolution = make_test_resolution(&registries, &mut loaded, source);
    let before = loaded.clone();

    assert_eq!(
        validate_start_process(&registries, &loaded, &resolution, source, destination).err(),
        Some(StartProcessError::StructureRevisionExhausted)
    );
    assert_eq!(loaded, before);
}

#[test]
fn trusted_load_rejects_supported_output_without_structure_completion_revision_capacity() {
    let (registries, mut state, source, destination) = unstarted_process_fixture();
    let support = add_active_stockpile_support(&registries, &mut state, 0);
    let _ = validate_mount_stockpile(&registries, &state, destination, support)
        .unwrap_or_else(|error| panic!("production supported-load mount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("production supported-load mount commit failed: {error}"));
    let resolution = make_test_resolution(&registries, &mut state, source);
    let token = validate_start_process(&registries, &state, &resolution, source, destination)
        .unwrap_or_else(|error| panic!("production supported-load validation failed: {error}"));
    let _ = commit_process_for_test(token, &mut state);

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("production supported-load serialization failed: {error}"));
    encoded["state"]["systems"]["structures"]["revision"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("production supported-load decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(
            StateValidationError::ProductionStructureRevisionCapacityExhausted {
                revision: u64::MAX,
                completion_buckets: 1,
            }
        ))
    );
}

#[test]
fn process_start_preserves_structure_revisions_owed_to_existing_supported_output_buckets() {
    let registries = make_test_registries();
    let mut state = AppState::new();
    let first_source = add_test_stockpile(&mut state, 100);
    let first_destination = add_test_stockpile(&mut state, 100);
    let second_source = add_test_stockpile(&mut state, 100);
    let second_destination = add_test_stockpile(&mut state, 100);
    deposit_test_wood(&registries, &mut state, first_source, 20);
    deposit_test_wood(&registries, &mut state, second_source, 20);
    let first_support = add_active_stockpile_support(&registries, &mut state, 0);
    let second_support = add_active_stockpile_support(&registries, &mut state, 2);
    let _ = validate_mount_stockpile(&registries, &state, first_destination, first_support)
        .unwrap_or_else(|error| panic!("first supported destination mount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("first supported destination mount commit failed: {error}"));
    let _ = validate_mount_stockpile(&registries, &state, second_destination, second_support)
        .unwrap_or_else(|error| panic!("second supported destination mount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("second supported destination mount commit failed: {error}")
        });
    let first_resources = add_test_heating_resources(&registries, &mut state);
    let second_resources = add_test_heating_resources(&registries, &mut state);

    let first_resolution = resolve_test_heating(
        &registries,
        &state,
        TEST_PROCESS,
        first_source,
        first_resources,
        TEST_TARGET_TEMPERATURE,
    );
    let first = validate_start_process(
        &registries,
        &state,
        &first_resolution,
        first_source,
        first_destination,
    )
    .unwrap_or_else(|error| panic!("existing supported production validation failed: {error}"));
    let first_job = commit_process_for_test(first, &mut state);
    let first_due = state
        .production()
        .get_job(first_job)
        .map(ProductionJobRecord::completes_at)
        .unwrap_or_else(|| panic!("existing supported production job disappeared"));
    let _ = advance_tick(&registries, &mut state).unwrap_or_else(|error| {
        panic!("existing supported production pre-second-start tick failed: {error}")
    });
    assert!(
        state.tick() < first_due,
        "fixture requires the first supported production job to remain running after one tick"
    );

    let mut encoded =
        serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("production structure headroom serialization failed: {error}")
        });
    encoded["state"]["systems"]["structures"]["revision"] = serde_json::json!(u64::MAX - 1);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("production structure headroom decode failed: {error}"));
    let loaded = decoded.into_state(&registries).unwrap_or_else(|error| {
        panic!("one supported completion must fit at structural revision MAX-1: {error}")
    });
    let second_resolution = resolve_test_heating(
        &registries,
        &loaded,
        TEST_PROCESS,
        second_source,
        second_resources,
        TEST_TARGET_TEMPERATURE,
    );
    let before = loaded.clone();

    assert_eq!(
        validate_start_process(
            &registries,
            &loaded,
            &second_resolution,
            second_source,
            second_destination,
        )
        .err(),
        Some(StartProcessError::StructureRevisionExhausted)
    );
    assert_eq!(loaded, before);
}
