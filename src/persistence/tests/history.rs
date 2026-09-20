//! Material-history, identity-cursor, and structural-embodiment load rejection contracts.

use super::*;

#[test]
fn future_material_storage_transition_is_rejected_on_load() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5700_0028));
    let stockpile = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(10))
        .unwrap_or_else(|error| panic!("storage-history stockpile fixture failed: {error}"));
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(1),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("storage-history lot fixture failed: {error}"));
    let current = state.tick();
    let future = SimulationTick::new(current.value() + 1);
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("storage-history tamper serialization failed: {error}"));
    encoded["state"]["systems"]["inventory"]["lots"][lot.value().to_string()]["storage_history"]
        ["last_transition_at"] = serde_json::json!(future.value());
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("storage-history tamper decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Inventory(
            InventoryValidationError::LotStorageTransitionInFuture {
                lot,
                transition: future,
                current,
            }
        )))
    );
}

#[test]
fn impossible_material_history_times_are_rejected_on_load() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5700_0029));
    apply_clock_advance(&mut state, SimulationTick::new(2));
    let stockpile = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(10))
        .unwrap_or_else(|error| panic!("material-history stockpile fixture failed: {error}"));
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(1),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("material-history lot fixture failed: {error}"));
    let base = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("material-history serialization failed: {error}"));

    let mut future_provenance = base.clone();
    future_provenance["state"]["systems"]["inventory"]["lots"][lot.value().to_string()]["provenance"]
        ["latest_created_at"] = serde_json::json!(3_u64);
    let future_provenance: LoadedSaveEnvelope = serde_json::from_value(future_provenance)
        .unwrap_or_else(|error| panic!("future provenance decode failed: {error}"));
    assert_eq!(
        future_provenance.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Inventory(
            InventoryValidationError::LotProvenanceInFuture {
                lot,
                latest: SimulationTick::new(3),
                current: SimulationTick::new(2),
            }
        )))
    );

    let mut early_transition = base;
    early_transition["state"]["systems"]["inventory"]["lots"][lot.value().to_string()]["storage_history"]
        ["last_transition_at"] = serde_json::json!(1_u64);
    let early_transition: LoadedSaveEnvelope = serde_json::from_value(early_transition)
        .unwrap_or_else(|error| panic!("early storage-transition decode failed: {error}"));
    assert_eq!(
        early_transition.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Inventory(
            InventoryValidationError::LotStorageTransitionBeforeCreation {
                lot,
                transition: SimulationTick::new(1),
                created: SimulationTick::new(2),
            }
        )))
    );
}

#[test]
fn rewound_inventory_identity_cursors_are_rejected_on_load() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5700_0030));
    let stockpile = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(10))
        .unwrap_or_else(|error| panic!("cursor-tamper stockpile fixture failed: {error}"));
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(1),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("cursor-tamper lot fixture failed: {error}"));
    let base = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("cursor-tamper serialization failed: {error}"));

    let mut stockpile_cursor = base.clone();
    stockpile_cursor["state"]["systems"]["inventory"]["next_stockpile_id"] =
        serde_json::json!(stockpile.value());
    let stockpile_cursor: LoadedSaveEnvelope = serde_json::from_value(stockpile_cursor)
        .unwrap_or_else(|error| panic!("stockpile cursor tamper decode failed: {error}"));
    assert_eq!(
        stockpile_cursor.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Inventory(
            InventoryValidationError::NextIdNotAfterExisting {
                next: stockpile.value(),
                highest: stockpile,
            }
        )))
    );

    let mut lot_cursor = base;
    lot_cursor["state"]["systems"]["inventory"]["next_lot_id"] = serde_json::json!(lot.value());
    let lot_cursor: LoadedSaveEnvelope = serde_json::from_value(lot_cursor)
        .unwrap_or_else(|error| panic!("lot cursor tamper decode failed: {error}"));
    assert_eq!(
        lot_cursor.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Inventory(
            InventoryValidationError::NextLotIdNotAfterExisting {
                next: lot.value(),
                highest: lot,
            }
        )))
    );
}

#[test]
fn tampered_structural_unauthored_embodiment_is_rejected_on_load() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5700_0018));
    let member = make_test_structural_element(&registries, &mut state, 0, 0, true);
    let molten_wood = CommodityKey::new(MATERIAL_WOOD, FORM_MOLTEN);
    let mut encoded = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("structural liquid-embodiment save serialization failed: {error}"),
    };
    encoded["state"]["systems"]["structures"]["elements"][member.value().to_string()]["embodied_material"]
        [0]["profile"]["commodity"] = serde_json::json!(molten_wood.value());
    let decoded: LoadedSaveEnvelope = match serde_json::from_value(encoded) {
        Ok(decoded) => decoded,
        Err(error) => {
            panic!("tampered structural liquid-embodiment save failed decode: {error}")
        }
    };

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Structure(
            StructureValidationError::UnknownEmbodiedCommodity { element: member }
        )))
    );
}
