//! Contracts for active carried-equipment selection and persistence.

use super::*;

use crate::content::{
    EQUIPMENT_STONE_PICK, FORM_HANDLE, FORM_TOOL, MATERIAL_STONE, MATERIAL_WOOD, build_registries,
};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::{AppState, StateValidationError};
use crate::equipment::validate_assemble_equipment;
use crate::inventory::{add_solid_stockpile_for_test, deposit_lot_for_test};
use crate::logistics::{
    LogisticsValidationError, place_equipment_for_fixture, validate_drop_equipment,
    validate_pickup_equipment,
};
use crate::material::CommodityKey;
use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::spatial::VoxelCoord;

const ORIGIN: VoxelCoord = VoxelCoord::new(0, 0, 0);

fn selected_pick_fixture() -> (
    crate::registry::Registries,
    AppState,
    crate::equipment::EquipmentId,
) {
    let registries = build_registries();
    let mut state = AppState::new();
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000))
        .unwrap_or_else(|error| panic!("selection assembly stockpile failed: {error}"));
    for (commodity, mass) in [
        (
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            Mass::from_milligrams(800_000),
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            Mass::from_milligrams(200_000),
        ),
    ] {
        deposit_lot_for_test(
            &registries,
            &mut state,
            source,
            commodity,
            mass,
            Temperature::from_millikelvin(293_150),
        )
        .unwrap_or_else(|error| panic!("selection assembly material failed: {error}"));
    }
    let pick = validate_assemble_equipment(&registries, &state, EQUIPMENT_STONE_PICK, source)
        .unwrap_or_else(|error| panic!("selection pick assembly failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("selection pick assembly commit failed: {error}"));
    place_equipment_for_fixture(&mut state, pick, ORIGIN);
    validate_admit_player(&registries, &state, ORIGIN)
        .unwrap_or_else(|error| panic!("selection player admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("selection player admission commit failed: {error}"));
    validate_pickup_equipment(&registries, &state, pick)
        .unwrap_or_else(|error| panic!("selection pickup failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("selection pickup commit failed: {error}"));
    (registries, state, pick)
}

#[test]
fn active_equipment_is_a_persisted_reference_into_carried_custody() {
    let (registries, mut state, pick) = selected_pick_fixture();
    assert_eq!(state.logistics().active_equipment(), None);

    validate_set_active_equipment(&state, Some(pick))
        .unwrap_or_else(|error| panic!("active-equipment validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("active-equipment commit failed: {error}"));
    assert_eq!(state.logistics().active_equipment(), Some(pick));

    let revision = state.logistics().revision();
    validate_set_active_equipment(&state, Some(pick))
        .unwrap_or_else(|error| panic!("idempotent active-equipment validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("idempotent active-equipment commit failed: {error}"));
    assert_eq!(state.logistics().revision(), revision);

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("active-equipment serialization failed: {error}"));
    let loaded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("active-equipment decode failed: {error}"));
    let loaded = loaded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("active-equipment trusted load failed: {error}"));
    assert_eq!(loaded, state);
    assert_eq!(loaded.logistics().active_equipment(), Some(pick));

    validate_drop_equipment(&registries, &state, pick)
        .unwrap_or_else(|error| panic!("selected-equipment drop failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("selected-equipment drop commit failed: {error}"));
    assert_eq!(state.logistics().active_equipment(), None);
}

#[test]
fn active_equipment_rejects_noncarried_selection_and_forged_persistence() {
    let (registries, mut state, pick) = selected_pick_fixture();
    validate_drop_equipment(&registries, &state, pick)
        .unwrap_or_else(|error| panic!("noncarried selection drop failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("noncarried selection drop commit failed: {error}"));
    assert_eq!(
        validate_set_active_equipment(&state, Some(pick)).err(),
        Some(PlayerEquipmentSelectionError::EquipmentNotCarried { equipment: pick })
    );

    validate_pickup_equipment(&registries, &state, pick)
        .unwrap_or_else(|error| panic!("forged-selection pickup failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("forged-selection pickup commit failed: {error}"));
    validate_set_active_equipment(&state, Some(pick))
        .unwrap_or_else(|error| panic!("forged-selection baseline validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("forged-selection baseline commit failed: {error}"));
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("forged-selection serialization failed: {error}"));
    encoded["state"]["systems"]["logistics"]["carried_equipment"] = serde_json::json!([]);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("forged-selection decode failed: {error}"));
    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Logistics(
            LogisticsValidationError::ActiveEquipmentNotCarried { equipment: pick }
        )))
    );
}
