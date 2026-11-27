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
fn trusted_load_rejects_running_production_without_future_completion_revision_capacity() {
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
            ProductionValidationError::FutureCompletionRevisionCapacityExhausted {
                revision: u64::MAX,
                required: 1,
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
                required: 1,
            }
        ))
    );
}

#[test]
fn trusted_load_reserves_distinct_inventory_revisions_for_same_tick_jobs() {
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
    .unwrap_or_else(|error| panic!("same-tick first production validation failed: {error}"));
    let first_job = commit_process_for_test(first, &mut state);
    let second_resolution = make_test_resolution(&registries, &mut state, second_source);
    let second = validate_start_process(
        &registries,
        &state,
        &second_resolution,
        second_source,
        second_destination,
    )
    .unwrap_or_else(|error| panic!("same-tick second production validation failed: {error}"));
    let second_job = commit_process_for_test(second, &mut state);
    let first_due = state
        .production()
        .get_job(first_job)
        .map(ProductionJobRecord::completes_at)
        .unwrap_or_else(|| panic!("same-tick first production job disappeared"));
    let second_due = state
        .production()
        .get_job(second_job)
        .map(ProductionJobRecord::completes_at)
        .unwrap_or_else(|| panic!("same-tick second production job disappeared"));
    assert_eq!(
        first_due, second_due,
        "fixture must share one current due tick"
    );

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("same-tick headroom serialization failed: {error}"));
    encoded["state"]["systems"]["inventory"]["revision"] = serde_json::json!(u64::MAX - 1);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("same-tick headroom decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(
            StateValidationError::ProductionInventoryRevisionCapacityExhausted {
                revision: u64::MAX - 1,
                required: 2,
            }
        )),
        "same-tick batching cannot collapse durable headroom because later suspension may separate the jobs"
    );
}

#[test]
fn process_start_preserves_inventory_revisions_owed_to_existing_jobs() {
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
        panic!("one retained completion must fit at inventory revision MAX-2: {error}")
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
        validate_material_relocation_for_test(
            &registries,
            &loaded,
            unrelated_source,
            unrelated_destination,
            wood_log(),
            Mass::from_milligrams(1),
        )
        .err(),
        Some(MaterialRelocationTestError::Relocation(
            MaterialRelocationError::RevisionExhausted
        ))
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
                required: 1,
            }
        ))
    );
}

#[test]
fn process_start_preserves_equipment_revisions_owed_to_existing_wear_jobs() {
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
        panic!("one retained wear completion must fit at equipment revision MAX-1: {error}")
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
                required: 1,
            }
        ))
    );
}

#[test]
fn earlier_independent_actions_cannot_spend_revisions_reserved_by_later_production() {
    use crate::content::{
        ENERGY_ELECTRICAL_BUFFER, EQUIPMENT_ELECTRIC_FURNACE, EQUIPMENT_WOODEN_CLUB, FLUID_WATER,
        PROCESS_HEAT_MATERIAL_BATCH, STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER, build_registries,
    };
    use crate::core::quantity::Volume;
    use crate::equipment::{
        EquipmentSupportCommitError, validate_assemble_equipment, validate_mount_equipment,
    };
    use crate::fluid::{
        FluidSupportCommitError, FluidTransferCommitError,
        add_fluid_store_with_contents_for_fixture, validate_mount_fluid_store,
        validate_transfer_fluid_between_local_stores,
    };
    use crate::logistics::{
        place_energy_store_for_fixture, place_equipment_for_fixture, validate_place_fluid_store,
    };
    use crate::matter::calculate_matter_accounting;
    use crate::structural::{
        GroundedStructuralElementPlan, PlanPlayerStructuralElementCommitError,
        PlayerStructuralConstructionCommitError, StructuralCommitError, StructuralElementGeometry,
        validate_plan_player_grounded_structural_element,
        validate_start_player_structural_construction,
    };
    use crate::survival::{DrinkCommitError, validate_drink};

    let registries = build_registries();
    let mut state = AppState::new();
    let origin = VoxelCoord::new(0, 0, 0);
    let source = add_test_stockpile(&mut state, 2_000_000);
    let destination = add_test_stockpile(&mut state, 100_000);
    let mass = Mass::from_milligrams(10_000);
    let room = Temperature::from_millikelvin(293_150);
    deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        wood_log(),
        Mass::from_milligrams(1_000_000),
        room,
    )
    .unwrap();
    let spare = validate_assemble_equipment(&registries, &state, EQUIPMENT_WOODEN_CLUB, source)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let lot =
        deposit_lot_for_test(&registries, &mut state, source, wood_log(), mass, room).unwrap();
    let furnace = add_equipment(
        &registries,
        &mut state,
        EQUIPMENT_ELECTRIC_FURNACE,
        Condition::PRISTINE,
    )
    .unwrap();
    let energy = add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        ENERGY_ELECTRICAL_BUFFER,
        Energy::from_nanojoules(2_000_000_000_000_000),
    )
    .unwrap();
    let support = add_active_stockpile_support(&registries, &mut state, 0);
    let _ = validate_mount_equipment(&registries, &state, furnace, support)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let member = add_structural_element(
        &registries,
        &mut state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        StructuralElementGeometry::new(
            VoxelBounds::new(VoxelCoord::new(0, 0, 1), VoxelCoord::new(1, 1, 2)).unwrap(),
            Length::from_micrometers(2_000_000),
            Area::from_square_millimeters(5_000),
        )
        .unwrap(),
        true,
    )
    .unwrap();
    materialize_structural_element_for_test(&registries, &mut state, member, FORM_LOG);
    for stockpile in [source, destination] {
        validate_place_ground_stockpile(&state, stockpile, origin)
            .unwrap()
            .commit(&mut state)
            .unwrap();
    }
    let _ = validate_mount_stockpile(&registries, &state, destination, support)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    for equipment in [furnace, spare] {
        place_equipment_for_fixture(&mut state, equipment, origin);
    }
    place_energy_store_for_fixture(&mut state, energy, origin);
    let water = Volume::from_microliters(100_000);
    let donor = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        water,
        FLUID_WATER,
        water,
        room,
    )
    .unwrap();
    let receiver = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        Volume::from_microliters(200_000),
        FLUID_WATER,
        water,
        room,
    )
    .unwrap();
    for store in [donor, receiver] {
        validate_place_fluid_store(&state, store, origin)
            .unwrap()
            .commit(&mut state)
            .unwrap();
    }
    let _ = validate_mount_fluid_store(&registries, &state, receiver, support)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    validate_admit_player(&registries, &state, origin)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let mut saved = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    saved["state"]["systems"]["structures"]["revision"] = serde_json::json!(u64::MAX - 1);
    saved["state"]["systems"]["equipment"]["revision"] = serde_json::json!(u64::MAX - 1);
    let mut state = serde_json::from_value::<LoadedSaveEnvelope>(saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();

    let plan = validate_plan_player_grounded_structural_element(
        &registries,
        &state,
        GroundedStructuralElementPlan::new(
            STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
            MATERIAL_WOOD,
            StructuralElementGeometry::new(
                VoxelBounds::new(VoxelCoord::new(1, 0, 0), VoxelCoord::new(2, 1, 1)).unwrap(),
                Length::from_micrometers(2_000_000),
                Area::from_square_millimeters(5_000),
            )
            .unwrap(),
        ),
    )
    .unwrap();
    let equipment_mount = validate_mount_equipment(&registries, &state, spare, support).unwrap();
    let construction = validate_start_player_structural_construction(
        &registries,
        &state,
        STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
        member,
        &[],
    )
    .unwrap();
    let fluid_mount = validate_mount_fluid_store(&registries, &state, donor, support).unwrap();
    let pour =
        validate_transfer_fluid_between_local_stores(&registries, &state, donor, receiver, water)
            .unwrap();
    let drink = validate_drink(&registries, &state, receiver, water).unwrap();
    let matter = calculate_matter_accounting(&state).unwrap().total();
    let resolution = resolve_sensible_heating_process(
        &registries,
        &state,
        SensibleHeatingRequest::new(
            PROCESS_HEAT_MATERIAL_BATCH,
            source,
            &[MaterialLotSelection::new(lot, mass)],
            furnace,
            energy,
            Temperature::from_millikelvin(303_150),
        ),
    )
    .unwrap();
    let job = validate_start_player_process(
        &registries,
        &state,
        resolution.process_resolution(),
        source,
        destination,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    assert_eq!(state.checked_future_structure_revision_demand(), Some(1));
    assert_eq!(state.checked_future_equipment_revision_demand(), Some(1));
    assert_eq!(state.structures().revision(), u64::MAX - 1);
    assert_eq!(state.equipment().revision(), u64::MAX - 1);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let mut candidate = state.clone();
    assert_eq!(
        construction.commit(&mut candidate),
        Err(PlayerStructuralConstructionCommitError::Work(
            crate::labor::PlayerWorkCommitError::StructureRevisionExhausted,
        )),
        "previously validated construction cannot reserve the revision already owed to production",
    );
    assert_eq!(candidate, state);
    assert_eq!(
        plan.commit(&mut candidate).err(),
        Some(PlanPlayerStructuralElementCommitError::RevisionExhausted),
    );
    assert_eq!(candidate, state);
    assert_eq!(
        equipment_mount.commit(&mut candidate).err(),
        Some(EquipmentSupportCommitError::EquipmentRevisionExhausted),
    );
    assert_eq!(candidate, state);
    assert_eq!(
        fluid_mount.commit(&mut candidate).err(),
        Some(FluidSupportCommitError::Structure(
            StructuralCommitError::RevisionExhausted
        )),
    );
    assert_eq!(candidate, state);
    assert_eq!(
        pour.commit(&mut candidate).err(),
        Some(FluidTransferCommitError::Structure(
            StructuralCommitError::RevisionExhausted
        )),
    );
    assert_eq!(candidate, state);
    assert_eq!(
        drink.commit(&mut candidate).err(),
        Some(DrinkCommitError::Structure(
            StructuralCommitError::RevisionExhausted
        )),
    );
    assert_eq!(candidate, state);
    let due = state.production().get_job(job).unwrap().completes_at();
    while state.tick() < due {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(state.equipment().revision(), u64::MAX);
    assert_eq!(state.structures().revision(), u64::MAX);
    assert!(state.production().get_job(job).is_none());
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), matter);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn process_start_preserves_structure_revisions_owed_to_existing_supported_output_jobs() {
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
        panic!("one retained supported completion must fit at structural revision MAX-1: {error}")
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
