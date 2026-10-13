//! Mining admission, settlement headroom, and destination ownership contracts.

use super::*;

#[test]
fn destination_capacity_rejection_does_not_reveal_short_hidden_reserve() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("non-oracular mining survival setup failed: {error}"));
    let pick = assemble_pick_for_test(&registries, &mut state);
    let destination_capacity = Mass::from_milligrams(60_000);
    let destination = add_solid_stockpile_for_test(&mut state, destination_capacity)
        .unwrap_or_else(|error| panic!("non-oracular mining destination failed: {error}"));
    let hidden_reserve = Mass::from_milligrams(50_000);
    let deposit = insert_known_deposit(
        &registries,
        &mut state,
        deposit_spec_with_mass(hidden_reserve),
    )
    .unwrap_or_else(|error| panic!("non-oracular mining deposit failed: {error}"));
    let requested = Mass::from_milligrams(100_000);
    let before = state.clone();

    assert!(hidden_reserve < destination_capacity);
    assert_eq!(
        validate_known_mining(
            &registries,
            &state,
            MINING_METHOD_HAND_PICK,
            deposit,
            destination,
            pick,
            requested,
        )
        .err(),
        Some(MiningStartError::DestinationCapacityExceeded {
            stockpile: destination,
            capacity: destination_capacity,
            committed: Mass::ZERO,
            requested,
        })
    );
    assert_eq!(state, before);
}

#[test]
fn mining_destination_validation_does_not_probe_hidden_output_temperature() {
    for hidden_temperature in [300_000_u32, 500_000_u32] {
        let registries = build_registries();
        let mut state = AppState::new();
        initialize_player_survival(&registries, &mut state)
            .unwrap_or_else(|error| panic!("temperature-oracle survival setup failed: {error}"));
        let pick = assemble_pick_for_test(&registries, &mut state);
        let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100_000))
            .unwrap_or_else(|error| panic!("temperature-oracle destination failed: {error}"));
        let construction =
            add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2_400_000))
                .unwrap_or_else(|error| {
                    panic!("temperature-oracle construction stockpile failed: {error}")
                });
        deposit_lot_for_test(
            &registries,
            &mut state,
            construction,
            CommodityKey::new(MATERIAL_WOOD, FORM_CHEST_BODY),
            Mass::from_milligrams(2_400_000),
            Temperature::from_millikelvin(293_150),
        )
        .unwrap_or_else(|error| panic!("temperature-oracle chest body failed: {error}"));
        validate_build_storage_enclosure(
            &registries,
            &state,
            STORAGE_TIMBER_PROVISIONS_CHEST,
            destination,
            construction,
        )
        .unwrap_or_else(|error| panic!("temperature-oracle enclosure build failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("temperature-oracle enclosure commit failed: {error}"));
        let maximum = state
            .inventory()
            .get_stockpile(destination)
            .unwrap_or_else(|| panic!("temperature-oracle destination disappeared"))
            .storage_profile()
            .maximum_temperature();
        let base = deposit_spec_with_mass(Mass::from_milligrams(100_000));
        let deposit = insert_known_deposit(
            &registries,
            &mut state,
            GeneratedDepositSpec::new(
                base.bounds(),
                base.commodity(),
                base.mass(),
                Temperature::from_millikelvin(hidden_temperature),
                base.excavation_hardness(),
                base.composition().clone(),
            )
            .unwrap_or_else(|error| panic!("temperature-oracle deposit spec failed: {error}")),
        )
        .unwrap_or_else(|error| panic!("temperature-oracle deposit failed: {error}"));
        let before = state.clone();

        let error = validate_known_mining(
            &registries,
            &state,
            MINING_METHOD_HAND_PICK,
            deposit,
            destination,
            pick,
            Mass::from_milligrams(100_000),
        )
        .err()
        .unwrap_or_else(|| panic!("temperature-limited mining destination unexpectedly admitted"));
        assert_eq!(
            error,
            MiningStartError::DestinationTemperatureNotAcquired {
                stockpile: destination,
                maximum,
            }
        );
        assert!(
            !error.to_string().contains(&hidden_temperature.to_string()),
            "mining destination rejection must not disclose hidden output temperature"
        );
        assert_eq!(state, before);
    }
}

#[test]
fn mining_cannot_reserve_output_into_an_active_dismantling_target() {
    let (registries, mut state, deposit, destination, pick) = survival_only_mining_fixture();
    let construction = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2_400_000))
        .unwrap_or_else(|error| panic!("mining dismantle construction stockpile failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        construction,
        CommodityKey::new(MATERIAL_WOOD, FORM_CHEST_BODY),
        Mass::from_milligrams(2_400_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("mining dismantle enclosure body failed: {error}"));
    validate_build_storage_enclosure(
        &registries,
        &state,
        STORAGE_TIMBER_PROVISIONS_CHEST,
        destination,
        construction,
    )
    .unwrap_or_else(|error| panic!("mining dismantle enclosure build failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("mining dismantle enclosure build commit failed: {error}"));
    let recovery = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2_400_000))
        .unwrap_or_else(|error| panic!("mining dismantle recovery stockpile failed: {error}"));
    let _ =
        validate_start_storage_enclosure_dismantling(&registries, &state, destination, recovery)
            .unwrap_or_else(|error| panic!("mining dismantle start failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("mining dismantle start commit failed: {error}"));
    let before = state.clone();

    assert_eq!(
        validate_known_mining(
            &registries,
            &state,
            MINING_METHOD_HAND_PICK,
            deposit,
            destination,
            pick,
            Mass::from_milligrams(1_000),
        )
        .err(),
        Some(MiningStartError::DestinationBusyStorageDismantling {
            stockpile: destination,
        })
    );
    assert_eq!(state, before);
}

#[test]
fn public_mining_debug_does_not_expose_hidden_source_or_reserved_output() {
    let (registries, mut state, deposit, destination, pick) = survival_only_mining_fixture();
    let job = validate_known_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        deposit,
        destination,
        pick,
        Mass::from_milligrams(1_000),
    )
    .unwrap_or_else(|error| panic!("mining debug start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("mining debug commit failed: {error}"));

    let job_debug = format!(
        "{:?}",
        state
            .mining()
            .get_job(job)
            .unwrap_or_else(|| panic!("mining debug job disappeared"))
    );
    let owner_debug = format!("{:?}", state.mining());

    for debug in [&job_debug, &owner_debug] {
        assert!(!debug.contains("deposit"));
        assert!(!debug.contains("deposit_mass_before"));
        assert!(!debug.contains("output"));
    }
}

fn awaiting_mining_settlement_fixture() -> (Registries, AppState, MiningJobId) {
    let (registries, mut state, deposit, destination, pick) = ordinary_unstarted_mining_fixture();
    let job = validate_known_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        deposit,
        destination,
        pick,
        Mass::from_milligrams(100_000),
    )
    .unwrap_or_else(|error| panic!("mining settlement exhaustion start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("mining settlement exhaustion start commit failed: {error}"));
    let _support = complete_mining_into_blocked_settlement_state(
        &registries,
        &mut state,
        job,
        destination,
        "mining settlement fixture",
    );
    let _ = validate_unmount_stockpile(&registries, &state, destination)
        .unwrap_or_else(|error| panic!("mining settlement fixture unmount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("mining settlement fixture unmount commit failed: {error}"));
    assert!(
        state
            .mining()
            .get_job(job)
            .is_some_and(MiningJobRecord::is_awaiting_settlement)
    );
    (registries, state, job)
}

#[test]
fn awaiting_settlement_is_the_only_current_serialized_phase_name() {
    let (registries, state, job) = awaiting_mining_settlement_fixture();
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("mining settlement phase serialization failed: {error}"));
    let phase = &mut encoded["state"]["systems"]["mining"]["jobs"][job.value().to_string()]["schedule"]
        ["phase"];
    assert_eq!(*phase, serde_json::json!("awaiting_settlement"));

    *phase = serde_json::json!("ready_to_claim");
    assert!(
        serde_json::from_value::<LoadedSaveEnvelope>(encoded).is_err(),
        "obsolete phase spelling must not deserialize in the current schema"
    );
}

#[test]
fn mining_start_rejects_exhausted_job_id_without_starting_work() {
    let (registries, state, deposit, destination, pick) = ordinary_unstarted_mining_fixture();
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("mining job-id exhaustion serialization failed: {error}"));
    encoded["state"]["systems"]["mining"]["next_job_id"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("mining job-id exhaustion decode failed: {error}"));
    let loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("mining job-id exhaustion fixture should load: {error}"));
    let before = loaded.clone();

    assert_eq!(
        validate_known_mining(
            &registries,
            &loaded,
            MINING_METHOD_HAND_PICK,
            deposit,
            destination,
            pick,
            Mass::from_milligrams(100_000),
        )
        .err(),
        Some(MiningStartError::MiningIdExhausted)
    );
    assert_eq!(loaded, before);
    assert_eq!(loaded.player_work().active(), None);
}

#[test]
fn mining_start_rejects_exhausted_mining_revision_without_starting_work() {
    let (registries, state, deposit, destination, pick) = ordinary_unstarted_mining_fixture();
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("mining revision exhaustion serialization failed: {error}"));
    encoded["state"]["systems"]["mining"]["revision"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("mining revision exhaustion decode failed: {error}"));
    let loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("mining revision exhaustion fixture should load: {error}"));
    let before = loaded.clone();

    assert_eq!(
        validate_known_mining(
            &registries,
            &loaded,
            MINING_METHOD_HAND_PICK,
            deposit,
            destination,
            pick,
            Mass::from_milligrams(100_000),
        )
        .err(),
        Some(MiningStartError::MiningRevisionExhausted)
    );
    assert_eq!(loaded, before);
    assert_eq!(loaded.player_work().active(), None);
}

#[test]
fn mining_start_reserves_completion_and_settlement_headroom() {
    let (registries, mut state, deposit, destination, pick) = ordinary_unstarted_mining_fixture();
    let position = state
        .logistics()
        .player()
        .map(|player| player.position())
        .unwrap_or_else(|| panic!("mining revision-budget player disappeared"));
    let support = active_stockpile_support_at(&registries, &mut state, position);
    let _ = validate_mount_stockpile(&registries, &state, destination, support)
        .unwrap_or_else(|error| panic!("mining revision-budget destination mount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("mining revision-budget destination mount commit failed: {error}")
        });
    let encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("mining revision-budget serialization failed: {error}"));

    for (owner, revision, expected) in [
        (
            "mining",
            u64::MAX - 2,
            MiningStartError::MiningRevisionExhausted,
        ),
        (
            "inventory",
            u64::MAX - 1,
            MiningStartError::InventoryRevisionExhausted,
        ),
        (
            "structures",
            u64::MAX,
            MiningStartError::StructureRevisionExhausted,
        ),
        (
            "geology",
            u64::MAX,
            MiningStartError::GeologyRevisionExhausted,
        ),
        (
            "equipment",
            u64::MAX,
            MiningStartError::EquipmentRevisionExhausted,
        ),
    ] {
        let mut candidate = encoded.clone();
        candidate["state"]["systems"][owner]["revision"] = serde_json::json!(revision);
        let decoded: LoadedSaveEnvelope = serde_json::from_value(candidate)
            .unwrap_or_else(|error| panic!("mining revision-budget decode failed: {error}"));
        let loaded = decoded.into_state(&registries).unwrap_or_else(|error| {
            panic!("idle near-exhausted mining owner should load: {error}")
        });
        let before = loaded.clone();

        assert_eq!(
            validate_known_mining(
                &registries,
                &loaded,
                MINING_METHOD_HAND_PICK,
                deposit,
                destination,
                pick,
                Mass::from_milligrams(100_000),
            )
            .err(),
            Some(expected)
        );
        assert_eq!(loaded, before);
    }

    let mut candidate = encoded;
    candidate["state"]["systems"]["inventory"]["next_lot_id"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(candidate)
        .unwrap_or_else(|error| panic!("mining lot-budget decode failed: {error}"));
    let loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("idle exhausted lot cursor should load: {error}"));
    assert_eq!(
        validate_known_mining(
            &registries,
            &loaded,
            MINING_METHOD_HAND_PICK,
            deposit,
            destination,
            pick,
            Mass::from_milligrams(100_000),
        )
        .err(),
        Some(MiningStartError::MaterialLotIdExhausted)
    );
}

#[test]
fn unmounted_mining_start_does_not_reserve_impossible_structural_work() {
    let (registries, state, deposit, destination, pick) = ordinary_unstarted_mining_fixture();
    let mut encoded =
        serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("unmounted mining structure-budget serialization failed: {error}")
        });
    encoded["state"]["systems"]["structures"]["revision"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("unmounted mining structure-budget decode failed: {error}"));
    let mut loaded = decoded.into_state(&registries).unwrap_or_else(|error| {
        panic!("unmounted mining at exhausted structural revision should load: {error}")
    });

    validate_known_mining(
        &registries,
        &loaded,
        MINING_METHOD_HAND_PICK,
        deposit,
        destination,
        pick,
        Mass::from_milligrams(100_000),
    )
    .unwrap_or_else(|error| {
        panic!("unmounted mining should not reserve a structural settlement revision: {error}")
    })
    .commit(&mut loaded)
    .unwrap_or_else(|error| panic!("unmounted mining start commit failed: {error}"));

    assert_eq!(loaded.structures().revision(), u64::MAX);
    assert_eq!(validate_loaded_state(&registries, &loaded), Ok(()));
}

#[test]
fn trusted_load_rejects_working_mining_without_scheduled_completion_revisions() {
    let (registries, mut state, deposit, destination, pick) = survival_only_mining_fixture();
    let job = validate_known_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        deposit,
        destination,
        pick,
        Mass::from_milligrams(100_000),
    )
    .unwrap_or_else(|error| panic!("mining load revision-budget start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("mining load revision-budget commit failed: {error}"));
    let encoded =
        serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("mining load revision-budget serialization failed: {error}")
        });

    for (owner, expected) in [
        (
            "mining",
            MiningJobValidationError::WorkingMiningRevisionExhausted { job },
        ),
        (
            "geology",
            MiningJobValidationError::WorkingGeologyRevisionExhausted { job },
        ),
        (
            "equipment",
            MiningJobValidationError::WorkingEquipmentRevisionExhausted { job },
        ),
    ] {
        let mut candidate = encoded.clone();
        candidate["state"]["systems"][owner]["revision"] = serde_json::json!(u64::MAX);
        let decoded: LoadedSaveEnvelope = serde_json::from_value(candidate)
            .unwrap_or_else(|error| panic!("mining load revision-budget decode failed: {error}"));
        assert_eq!(
            decoded.into_state(&registries),
            Err(LoadError::InvalidState(StateValidationError::MiningJob(
                expected
            )))
        );
    }
}

#[test]
fn trusted_load_rejects_working_mining_without_settlement_headroom() {
    let (registries, mut state, deposit, destination, pick) = ordinary_unstarted_mining_fixture();
    let position = state
        .logistics()
        .player()
        .map(|player| player.position())
        .unwrap_or_else(|| panic!("headroom-bound mining player disappeared"));
    let support = active_stockpile_support_at(&registries, &mut state, position);
    let _ = validate_mount_stockpile(&registries, &state, destination, support)
        .unwrap_or_else(|error| {
            panic!("mining settlement-headroom destination mount failed: {error}")
        })
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("mining settlement-headroom destination mount commit failed: {error}")
        });
    let _job = validate_known_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        deposit,
        destination,
        pick,
        Mass::from_milligrams(100_000),
    )
    .unwrap_or_else(|error| panic!("mining settlement-headroom start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("mining settlement-headroom commit failed: {error}"));
    let encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("mining settlement-headroom serialization failed: {error}"));

    let mut candidate = encoded.clone();
    candidate["state"]["systems"]["inventory"]["next_lot_id"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(candidate)
        .unwrap_or_else(|error| panic!("mining settlement lot-headroom decode failed: {error}"));
    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(
            StateValidationError::FutureMaterialLotIdCapacityExhausted {
                next_lot_id: u64::MAX,
                required: 1,
            }
        ))
    );

    let mut candidate = encoded.clone();
    candidate["state"]["systems"]["inventory"]["revision"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(candidate).unwrap_or_else(|error| {
        panic!("mining settlement inventory-headroom decode failed: {error}")
    });
    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(
            StateValidationError::FutureInventoryRevisionCapacityExhausted {
                revision: u64::MAX,
                required: 1,
            }
        ))
    );

    let mut candidate = encoded.clone();
    candidate["state"]["systems"]["structures"]["revision"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(candidate).unwrap_or_else(|error| {
        panic!("mining settlement structure-headroom decode failed: {error}")
    });
    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(
            StateValidationError::FutureStructureRevisionCapacityExhausted {
                revision: u64::MAX,
                required: 1,
            }
        ))
    );

    let mut candidate = encoded;
    candidate["state"]["systems"]["mining"]["revision"] = serde_json::json!(u64::MAX - 1);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(candidate)
        .unwrap_or_else(|error| panic!("mining settlement owner-headroom decode failed: {error}"));
    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(
            StateValidationError::FutureMiningRevisionCapacityExhausted {
                revision: u64::MAX - 1,
                required: 2,
            }
        ))
    );
}

#[test]
fn trusted_load_rejects_awaiting_mining_output_without_settlement_lot_headroom() {
    let (registries, state, job) = awaiting_mining_settlement_fixture();
    let mut encoded =
        serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("mining settlement lot-id exhaustion serialization failed: {error}")
        });
    encoded["state"]["systems"]["inventory"]["next_lot_id"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded).unwrap_or_else(|error| {
        panic!("mining settlement lot-id exhaustion decode failed: {error}")
    });
    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(
            StateValidationError::FutureMaterialLotIdCapacityExhausted {
                next_lot_id: u64::MAX,
                required: 1,
            }
        )),
        "awaiting-settlement mining job {job:?} must retain one settlement identity"
    );
}

#[test]
fn trusted_load_rejects_awaiting_mining_output_without_settlement_inventory_revision() {
    let (registries, state, job) = awaiting_mining_settlement_fixture();
    let mut encoded =
        serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("mining settlement inventory revision exhaustion serialization failed: {error}")
        });
    encoded["state"]["systems"]["inventory"]["revision"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded).unwrap_or_else(|error| {
        panic!("mining settlement inventory revision exhaustion decode failed: {error}")
    });
    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(
            StateValidationError::FutureInventoryRevisionCapacityExhausted {
                revision: u64::MAX,
                required: 1,
            }
        )),
        "awaiting-settlement mining job {job:?} must retain one settlement inventory revision"
    );
}

#[test]
fn trusted_load_rejects_awaiting_mining_output_without_settlement_mining_revision() {
    let (registries, state, job) = awaiting_mining_settlement_fixture();
    let mut encoded =
        serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap_or_else(|error| {
            panic!("mining settlement mining revision exhaustion serialization failed: {error}")
        });
    encoded["state"]["systems"]["mining"]["revision"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded).unwrap_or_else(|error| {
        panic!("mining settlement mining revision exhaustion decode failed: {error}")
    });
    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(
            StateValidationError::FutureMiningRevisionCapacityExhausted {
                revision: u64::MAX,
                required: 1,
            }
        )),
        "awaiting-settlement mining job {job:?} must retain one settlement retirement revision"
    );
}

#[test]
fn mining_settlement_consumes_exact_reserved_headroom_at_owner_limits() {
    let (registries, mut state, job) = awaiting_mining_settlement_fixture();
    let destination = state
        .mining()
        .get_job(job)
        .map(MiningJobRecord::destination)
        .unwrap_or_else(|| panic!("headroom-bound mining job disappeared"));
    let position = state
        .logistics()
        .player()
        .map(|player| player.position())
        .unwrap_or_else(|| panic!("headroom-bound mining player disappeared"));
    let support = active_stockpile_support_at(&registries, &mut state, position);
    let _ = validate_mount_stockpile(&registries, &state, destination, support)
        .unwrap_or_else(|error| panic!("headroom-bound mining destination mount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("headroom-bound mining destination mount commit failed: {error}")
        });

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("headroom-bound mining serialization failed: {error}"));
    encoded["state"]["systems"]["inventory"]["next_lot_id"] = serde_json::json!(u64::MAX - 1);
    encoded["state"]["systems"]["inventory"]["revision"] = serde_json::json!(u64::MAX - 1);
    encoded["state"]["systems"]["mining"]["revision"] = serde_json::json!(u64::MAX - 1);
    encoded["state"]["systems"]["structures"]["revision"] = serde_json::json!(u64::MAX - 1);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("headroom-bound mining decode failed: {error}"));
    let mut loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("exact mining settlement headroom must load: {error}"));

    validate_mining_output_settlement_for_test(&registries, &loaded, job)
        .unwrap_or_else(|error| {
            panic!("exact mining settlement headroom rejected settlement: {error}")
        })
        .commit(&mut loaded)
        .unwrap_or_else(|error| panic!("exact mining settlement headroom commit failed: {error}"));

    assert_eq!(loaded.inventory().revision(), u64::MAX);
    assert_eq!(loaded.inventory().next_lot_id(), u64::MAX);
    assert_eq!(loaded.mining().revision(), u64::MAX);
    assert_eq!(loaded.structures().revision(), u64::MAX);
    assert!(loaded.mining().get_job(job).is_none());
    assert_eq!(validate_loaded_state(&registries, &loaded), Ok(()));
}

#[test]
fn mining_support_recovery_can_consume_released_settlement_headroom() {
    let (registries, mut state, deposit, destination, pick) = ordinary_unstarted_mining_fixture();
    let job = validate_known_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        deposit,
        destination,
        pick,
        Mass::from_milligrams(50_000),
    )
    .unwrap_or_else(|error| panic!("support-recovery mining start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("support-recovery mining start commit failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        destination,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(1),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("support-recovery destination ballast failed: {error}"));
    let _support = complete_mining_into_blocked_settlement_state(
        &registries,
        &mut state,
        job,
        destination,
        "support-recovery mining",
    );

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("support-recovery serialization failed: {error}"));
    encoded["state"]["systems"]["structures"]["revision"] = serde_json::json!(u64::MAX - 1);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("support-recovery decode failed: {error}"));
    let mut loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("reserved support-recovery state should load: {error}"));

    assert!(matches!(
        validate_mining_output_settlement_for_test(&registries, &loaded, job),
        Err(MiningSettlementError::StructuralLoad(
            StockpileStructuralLoadError::SupportNotActiveForIncrease { .. }
        ))
    ));
    let _ = validate_unmount_stockpile(&registries, &loaded, destination)
        .unwrap_or_else(|error| {
            panic!("unmount must consume the settlement's released structural headroom: {error}")
        })
        .commit(&mut loaded)
        .unwrap_or_else(|error| panic!("support-recovery unmount commit failed: {error}"));
    assert_eq!(loaded.structures().revision(), u64::MAX);

    validate_mining_output_settlement_for_test(&registries, &loaded, job)
        .unwrap_or_else(|error| panic!("unmounted recovered settlement rejected: {error}"))
        .commit(&mut loaded)
        .unwrap_or_else(|error| panic!("unmounted recovered settlement commit failed: {error}"));
    assert!(loaded.mining().get_job(job).is_none());
    assert_eq!(loaded.structures().revision(), u64::MAX);
    assert_eq!(validate_loaded_state(&registries, &loaded), Ok(()));
}

#[test]
fn mining_destination_mount_reserves_future_settlement_structure_headroom() {
    let (registries, mut state, job) = awaiting_mining_settlement_fixture();
    let destination = state
        .mining()
        .get_job(job)
        .map(MiningJobRecord::destination)
        .unwrap_or_else(|| panic!("mount-headroom mining job disappeared"));
    let position = state
        .logistics()
        .player()
        .map(|player| player.position())
        .unwrap_or_else(|| panic!("mount-headroom mining player disappeared"));
    let support = active_stockpile_support_at(&registries, &mut state, position);
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("mount-headroom serialization failed: {error}"));
    encoded["state"]["systems"]["structures"]["revision"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("mount-headroom decode failed: {error}"));
    let loaded = decoded.into_state(&registries).unwrap_or_else(|error| {
        panic!("unmounted settlement at structural limit should load: {error}")
    });

    assert!(matches!(
        validate_mount_stockpile(&registries, &loaded, destination, support),
        Err(crate::inventory::StockpileSupportError::Load(
            StockpileStructuralLoadError::Structure(
                crate::structural::StructuralMutationError::RevisionExhausted
            )
        ))
    ));
    assert!(
        validate_mining_output_settlement_for_test(&registries, &loaded, job).is_ok(),
        "unmounted settlement must remain landable when support assignment is rejected"
    );
}
