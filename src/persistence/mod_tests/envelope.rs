//! Save round-trip, schema-version, compatibility rejection, and unknown-field contracts.

use super::*;
use crate::logistics::validate_initialize_player_logistics;
use crate::player::validate_admit_player;
use crate::survival::initialize_player_survival;

#[test]
fn save_round_trip_preserves_authoritative_runtime_state() {
    let registries = build_registries();
    let mut state = AppState::new();
    let stockpile = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
        Ok(id) => id,
        Err(error) => panic!("fixture stockpile failed: {error}"),
    };
    if let Err(error) = deposit_bulk_for_test(
        &registries,
        &mut state,
        stockpile,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(25),
    ) {
        panic!("fixture deposit failed: {error}");
    }
    for _ in 0..37 {
        if let Err(error) = advance_tick(&registries, &mut state) {
            panic!("fixture tick unexpectedly failed: {error}");
        }
    }

    let encoded = match serde_json::to_vec(&SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("save serialization unexpectedly failed: {error}"),
    };
    let encoded_value: serde_json::Value = match serde_json::from_slice(&encoded) {
        Ok(encoded) => encoded,
        Err(error) => panic!("serialized save failed JSON inspection: {error}"),
    };
    let systems = &encoded_value["state"]["systems"];
    let inventory = &systems["inventory"];
    assert!(inventory.get("lot_indexes").is_none());
    assert!(inventory.get("stockpiles_by_support").is_none());
    assert!(systems["fluid"].get("stores_by_support").is_none());
    assert!(systems["equipment"].get("equipment_by_support").is_none());
    assert!(systems["structures"].get("dependents_by_support").is_none());
    assert!(
        systems["geological_knowledge"]
            .get("observations_by_material")
            .is_none()
    );
    for owner in ["production", "mining"] {
        assert!(systems[owner].get("due_jobs").is_none());
        assert!(systems[owner].get("equipment_occupancy").is_none());
    }
    assert!(systems["production"].get("energy_occupancy").is_none());
    assert!(
        systems["production"]
            .get("output_stockpile_occupancy")
            .is_none()
    );
    let stockpile_value = &inventory["stockpiles"][stockpile.value().to_string()];
    assert!(stockpile_value.get("lot_ids").is_none());
    let decoded: LoadedSaveEnvelope = match serde_json::from_slice(&encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("save deserialization unexpectedly failed: {error}"),
    };
    let loaded = match decoded.into_state(&registries) {
        Ok(loaded) => loaded,
        Err(error) => panic!("decoded save unexpectedly failed validation: {error}"),
    };

    assert_eq!(loaded, state);
    assert_eq!(loaded.inventory().lot_ids(stockpile).count(), 1);
    let reencoded = match serde_json::to_vec(&SaveEnvelope::new(&registries, &loaded)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("loaded save reserialization unexpectedly failed: {error}"),
    };
    assert_eq!(reencoded, encoded);
}

#[test]
fn partial_control_tick_progress_round_trips_exactly() {
    let registries = build_registries();
    let mut state = AppState::new();
    for expected in 1..=17 {
        let outcome = advance_control_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("control persistence fixture tick failed: {error}"));
        assert_eq!(outcome.control_tick(), ControlTick::new(expected));
        assert!(outcome.simulation().is_none());
    }
    assert_eq!(state.tick(), SimulationTick::ZERO);

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("control persistence serialization failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("control persistence decode failed: {error}"));
    let loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("control persistence validation failed: {error}"));

    assert_eq!(loaded, state);
    assert_eq!(loaded.control_tick(), ControlTick::new(17));
    assert_eq!(loaded.tick(), SimulationTick::ZERO);
}

#[test]
fn trusted_load_rejects_incoherent_multirate_clock() {
    let registries = build_registries();
    let state = AppState::new();
    let base = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("clock-tamper serialization failed: {error}"));

    let mut mismatch = base.clone();
    mismatch["state"]["clock"]["simulation_tick"] = serde_json::json!(1_u64);
    mismatch["state"]["clock"]["control_tick"] = serde_json::json!(17_u64);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(mismatch)
        .unwrap_or_else(|error| panic!("clock mismatch failed structural decode: {error}"));
    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(
            StateValidationError::ClockMismatch {
                control: ControlTick::new(17),
                stored_simulation: SimulationTick::new(1),
                derived_simulation: SimulationTick::ZERO,
            }
        ))
    );

    let invalid_control = ControlTick::MAX_COMPLETE
        .value()
        .checked_add(1)
        .unwrap_or_else(|| unreachable!("final complete control boundary is below u64::MAX"));
    let mut past_final_boundary = base;
    past_final_boundary["state"]["clock"]["simulation_tick"] =
        serde_json::json!(ControlTick::new(invalid_control).simulation_tick().value());
    past_final_boundary["state"]["clock"]["control_tick"] = serde_json::json!(invalid_control);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(past_final_boundary)
        .unwrap_or_else(|error| panic!("terminal clock tamper failed structural decode: {error}"));
    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(
            StateValidationError::ControlTickPastFinalBoundary {
                control: ControlTick::new(invalid_control),
                maximum: ControlTick::MAX_COMPLETE,
            }
        ))
    );
}

#[test]
fn trusted_load_rejects_partial_player_admission() {
    let registries = build_registries();

    let mut survival_only = AppState::new();
    initialize_player_survival(&registries, &mut survival_only)
        .unwrap_or_else(|error| panic!("survival-only persistence fixture failed: {error}"));
    assert_eq!(validate_loaded_state(&registries, &survival_only), Ok(()));
    assert_eq!(
        validate_persistable_state(&registries, &survival_only),
        Err(LoadError::IncompletePlayerAdmission {
            survival_initialized: true,
            logistics_initialized: false,
        })
    );
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &survival_only))
        .unwrap_or_else(|error| panic!("survival-only persistence serialization failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("survival-only persistence decode failed: {error}"));
    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::IncompletePlayerAdmission {
            survival_initialized: true,
            logistics_initialized: false,
        })
    );

    let mut logistics_only = AppState::new();
    validate_initialize_player_logistics(
        &logistics_only,
        VoxelCoord::new(0, 0, 0),
        Mass::from_milligrams(1),
    )
    .unwrap_or_else(|error| panic!("logistics-only persistence fixture failed: {error}"))
    .commit(&mut logistics_only)
    .unwrap_or_else(|error| panic!("logistics-only persistence commit failed: {error}"));
    assert_eq!(validate_loaded_state(&registries, &logistics_only), Ok(()));
    assert_eq!(
        validate_persistable_state(&registries, &logistics_only),
        Err(LoadError::IncompletePlayerAdmission {
            survival_initialized: false,
            logistics_initialized: true,
        })
    );
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &logistics_only))
        .unwrap_or_else(|error| panic!("logistics-only persistence serialization failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("logistics-only persistence decode failed: {error}"));
    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::IncompletePlayerAdmission {
            survival_initialized: false,
            logistics_initialized: true,
        })
    );
}

#[test]
fn atomic_player_admission_round_trips_through_trusted_load() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_admit_player(&registries, &state, VoxelCoord::new(4, -2, 7))
        .unwrap_or_else(|error| panic!("player persistence admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("player persistence admission commit failed: {error}"));

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("admitted player serialization failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("admitted player decode failed: {error}"));
    let loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("admitted player trusted load failed: {error}"));
    assert_eq!(loaded, state);
}

#[test]
fn trusted_load_rejects_tampered_admitted_player_carried_custody_shape() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_admit_player(&registries, &state, VoxelCoord::new(4, -2, 7))
        .unwrap_or_else(|error| panic!("player custody-tamper admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("player custody-tamper admission commit failed: {error}"));
    let carried = state
        .logistics()
        .player()
        .map(|player| player.carried_stockpile())
        .unwrap_or_else(|| panic!("player custody-tamper carried stockpile disappeared"));
    let expected_capacity = registries.survival().physiology().carried_mass_capacity();
    let encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("player custody-tamper serialization failed: {error}"));

    let mut capacity = encoded.clone();
    capacity["state"]["systems"]["inventory"]["stockpiles"][carried.value().to_string()]["capacity"] =
        serde_json::json!(expected_capacity.milligrams() + 1);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(capacity)
        .unwrap_or_else(|error| panic!("player capacity-tamper decode failed: {error}"));
    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::PlayerCarriedCapacityMismatch {
            stockpile: carried,
            expected: expected_capacity,
            actual: Mass::from_milligrams(expected_capacity.milligrams() + 1),
        })
    );

    let mut profile = encoded;
    profile["state"]["systems"]["inventory"]["stockpiles"][carried.value().to_string()]["storage_profile"]
        ["preservation_multiplier_ppm"] = serde_json::json!(2_000_000_u32);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(profile)
        .unwrap_or_else(|error| panic!("player profile-tamper decode failed: {error}"));
    assert!(matches!(
        decoded.into_state(&registries),
        Err(LoadError::PlayerCarriedStorageProfileMismatch {
            stockpile,
            expected: _,
            actual: _,
        }) if stockpile == carried
    ));
}

#[test]
fn unsupported_schema_is_rejected_before_runtime_use() {
    let encoded = br#"{
            "schema_version": 999,
            "registry_schema_version": 5,
            "state": {
                "clock": {"simulation_tick": 0, "control_tick": 0},
                "systems": {
                    "energy": {"revision": 0, "next_store_id": 1, "records": {}, "combusted_matter": {}},
                "fluid": {
                    "revision": 0,
                    "next_store_id": 1,
                    "records": {}
                },
                "equipment": {
                    "revision": 0,
                    "support_revision": 0,
                    "next_equipment_id": 1,
                    "records": {}
                },
                "structures": {
                    "revision": 0,
                    "next_element_id": 1,
                    "elements": {},
                    "supports_by_element": {}
                },
                "world": {"revision": 0, "chunks": []},
                "surface": {"revision": 0, "next_resource_id": 1, "resources": {}},
                "geology": {"revision": 0, "next_deposit_id": 1, "deposits": {}},
                "geological_knowledge": {
                    "revision": 0,
                    "next_observation_id": 1,
                    "observations": {}
                },
                "inventory": {
                    "revision": 0,
                    "support_revision": 0,
                    "next_stockpile_id": 1,
                    "next_lot_id": 1,
                    "stockpiles": {},
                    "lots": {}
                },
                "logistics": {
                    "revision": 0,
                    "player": null,
                    "traversable_edges": [],
                    "carried_equipment": [],
                    "active_equipment": null,
                    "stockpile_locations": {},
                    "equipment_locations": {},
                    "energy_store_locations": {},
                    "fluid_store_locations": {}
                },
                "production": {
                    "revision": 0,
                    "next_job_id": 1,
                    "jobs": {}
                },
                "mining": {
                    "revision": 0,
                    "next_job_id": 1,
                    "jobs": {}
                },
                "player_work": {
                    "revision": 0,
                    "active": null
                },
                "survival": {
                    "revision": 0,
                    "player": null,
                    "direct_consumption": {"pending": null},
                    "consumed_matter": {},
                    "consumed_fluids": {}
                }
                }
            }
        }"#;
    let registries = build_registries();
    let decoded: LoadedSaveEnvelope = match serde_json::from_slice(encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("fixture save failed to decode: {error}"),
    };

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::UnsupportedSchemaVersion {
            found: 999,
            supported: CURRENT_SAVE_SCHEMA_VERSION,
        })
    );
}

#[test]
fn support_revision_epochs_cannot_exceed_their_owner_revisions() {
    let registries = build_registries();
    let state = AppState::new();
    let base = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("support-revision fixture serialization failed: {error}"));

    let mut inventory = base.clone();
    inventory["state"]["systems"]["inventory"]["support_revision"] = serde_json::json!(1_u64);
    let inventory: LoadedSaveEnvelope = serde_json::from_value(inventory).unwrap_or_else(|error| {
        panic!("inventory support-revision fixture decode failed: {error}")
    });
    assert_eq!(
        inventory.into_state(&registries),
        Err(LoadError::InvalidState(
            crate::core::state::StateValidationError::Inventory(
                InventoryValidationError::SupportRevisionAfterRevision {
                    support_revision: 1,
                    revision: 0,
                }
            )
        ))
    );

    let mut equipment = base;
    equipment["state"]["systems"]["equipment"]["support_revision"] = serde_json::json!(1_u64);
    let equipment: LoadedSaveEnvelope = serde_json::from_value(equipment).unwrap_or_else(|error| {
        panic!("equipment support-revision fixture decode failed: {error}")
    });
    assert_eq!(
        equipment.into_state(&registries),
        Err(LoadError::InvalidState(
            crate::core::state::StateValidationError::Equipment(
                EquipmentValidationError::SupportRevisionAfterRevision {
                    support_revision: 1,
                    revision: 0,
                }
            )
        ))
    );
}

#[test]
fn previous_save_schema_is_rejected_without_compatibility_path() {
    let registries = build_registries();
    let state = AppState::new();
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("previous-schema fixture serialization failed: {error}"));
    let previous = CURRENT_SAVE_SCHEMA_VERSION
        .checked_sub(1)
        .unwrap_or_else(|| panic!("current save schema must have a previous positive version"));
    encoded["schema_version"] = serde_json::json!(previous);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("previous-schema fixture decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::UnsupportedSchemaVersion {
            found: previous,
            supported: CURRENT_SAVE_SCHEMA_VERSION,
        })
    );
}

#[test]
fn unknown_fields_are_rejected_at_envelope_and_nested_state_boundaries() {
    let registries = build_registries();
    let state = AppState::new();
    let base = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("strict-field fixture serialization failed: {error}"));

    let mut envelope = base.clone();
    envelope["unexpected"] = serde_json::json!(true);
    assert!(serde_json::from_value::<LoadedSaveEnvelope>(envelope).is_err());

    for path in [
        &["state"][..],
        &["state", "clock"],
        &["state", "random"],
        &["state", "systems"],
    ] {
        let mut nested = base.clone();
        let mut value = &mut nested;
        for segment in path {
            value = &mut value[*segment];
        }
        value["unexpected"] = serde_json::json!(true);
        assert!(
            serde_json::from_value::<LoadedSaveEnvelope>(nested).is_err(),
            "unknown field was accepted at persistent boundary {}",
            path.join(".")
        );
    }

    for owner in [
        "energy",
        "fluid",
        "equipment",
        "structures",
        "surface",
        "geology",
        "geological_knowledge",
        "inventory",
        "logistics",
        "production",
        "mining",
        "player_work",
        "survival",
    ] {
        let mut nested = base.clone();
        nested["state"]["systems"][owner]["unexpected"] = serde_json::json!(true);
        assert!(
            serde_json::from_value::<LoadedSaveEnvelope>(nested).is_err(),
            "unknown field was accepted by persistent owner {owner}"
        );
    }
}

#[test]
fn obsolete_player_voxel_payload_is_rejected_without_body_compatibility_defaults() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_admit_player(&registries, &state, VoxelCoord::new(2, 3, 4))
        .unwrap_or_else(|error| panic!("obsolete-player fixture admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("obsolete-player fixture commit failed: {error}"));
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("obsolete-player fixture serialization failed: {error}"));
    let player = encoded["state"]["systems"]["logistics"]["player"]
        .as_object_mut()
        .unwrap_or_else(|| panic!("serialized logistics player is not an object"));
    assert!(player.remove("body").is_some());
    player.insert(
        "position".to_owned(),
        serde_json::json!({"x": 2, "y": 3, "z": 4}),
    );

    assert!(
        serde_json::from_value::<LoadedSaveEnvelope>(encoded).is_err(),
        "current schema must not deserialize the removed voxel-only player representation"
    );
}
