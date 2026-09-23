//! Material-composition persistence contracts.

use super::*;

#[test]
fn mixed_composition_round_trip_preserves_constituents_exactly() {
    let registries = build_registries();
    let mut state = AppState::new();
    let stockpile = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
        Ok(id) => id,
        Err(error) => panic!("fixture stockpile failed: {error}"),
    };
    let composition = match MaterialComposition::new(vec![
        CompositionComponent::new(MATERIAL_COPPER, 720_000),
        CompositionComponent::new(MATERIAL_SLAG, 280_000),
    ]) {
        Ok(composition) => composition,
        Err(error) => panic!("composition fixture failed: {error}"),
    };
    let lot = match deposit_composed_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
        Mass::from_milligrams(25),
        Temperature::from_millikelvin(325_000),
        composition.clone(),
    ) {
        Ok(id) => id,
        Err(error) => panic!("composed lot fixture failed: {error}"),
    };

    let encoded = match serde_json::to_vec(&SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("mixed save serialization failed: {error}"),
    };
    let decoded: LoadedSaveEnvelope = match serde_json::from_slice(&encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("mixed save deserialization failed: {error}"),
    };
    let loaded = match decoded.into_state(&registries) {
        Ok(state) => state,
        Err(error) => panic!("mixed save validation failed: {error}"),
    };

    let loaded_lot = match loaded.inventory().get_lot(lot) {
        Some(lot) => lot,
        None => panic!("mixed lot disappeared after round trip"),
    };
    assert_eq!(loaded_lot.composition(), &composition);
    assert_eq!(loaded, state);
}

#[test]
fn unknown_lot_composition_constituent_is_rejected_on_load() {
    let registries = build_registries();
    let mut state = AppState::new();
    let stockpile = match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100)) {
        Ok(id) => id,
        Err(error) => panic!("fixture stockpile failed: {error}"),
    };
    let unknown = MaterialId::new(999_999);
    let lot = match deposit_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(295_000),
    ) {
        Ok(id) => id,
        Err(error) => panic!("lot fixture failed: {error}"),
    };

    let mut encoded = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
        Ok(encoded) => encoded,
        Err(error) => panic!("serialization failed: {error}"),
    };
    encoded["state"]["systems"]["inventory"]["lots"][lot.value().to_string()]["profile"]["composition"]
        ["components"] = serde_json::json!([
        {"material": MATERIAL_WOOD.value(), "parts_per_million": 900_000_u32},
        {"material": unknown.value(), "parts_per_million": 100_000_u32},
    ]);
    let decoded: LoadedSaveEnvelope = match serde_json::from_value(encoded) {
        Ok(decoded) => decoded,
        Err(error) => panic!("deserialization failed: {error}"),
    };

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Inventory(
            crate::inventory::InventoryValidationError::InvalidLotPhaseState {
                lot,
                error: crate::material::MaterialPhaseStateError::UnknownMaterial {
                    material: unknown,
                },
            }
        )))
    );
}
