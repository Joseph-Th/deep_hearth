//! Save round-trip, schema-version, compatibility rejection, and unknown-field contracts.

use super::*;

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
fn unsupported_schema_is_rejected_before_runtime_use() {
    let encoded = br#"{
            "schema_version": 999,
            "registry_schema_version": 5,
            "state": {
                "clock": {"tick": 0},
                "systems": {
                    "energy": {"revision": 0, "next_store_id": 1, "records": {}},
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
                    "ground_stockpiles": {},
                    "detached_equipment": {},
                    "detached_energy_stores": {},
                    "fluid_stores": {}
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
