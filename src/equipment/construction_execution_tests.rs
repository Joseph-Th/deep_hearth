//! Tests for the sibling construction execution module; isolated so test-only edits do not invalidate production builds.

use super::*;
use crate::content::{
    EQUIPMENT_STONE_PICK, FORM_HANDLE, FORM_TOOL, MATERIAL_STONE, MATERIAL_WOOD, build_registries,
};
use crate::core::quantity::Temperature;
use crate::core::state::StateValidationError;
use crate::core::time::WorldSeed;
use crate::equipment::EquipmentValidationError;
use crate::inventory::{add_solid_stockpile_for_test, deposit_lot_for_test};
use crate::material::CommodityKey;
use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};

#[test]
fn composite_pick_requires_both_authored_inputs_and_rejects_forged_embodiment() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xA55E_0001));
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000))
        .unwrap_or_else(|error| panic!("assembly source fixture failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
        Mass::from_milligrams(800_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("assembly stone fixture failed: {error}"));

    assert_eq!(
        validate_assemble_equipment(&registries, &state, EQUIPMENT_STONE_PICK, source).err(),
        Some(EquipmentAssemblyError::InsufficientMaterial {
            stockpile: source,
            available: Mass::ZERO,
            required: Mass::from_milligrams(200_000),
        })
    );
    deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
        Mass::from_milligrams(200_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("assembly handle fixture failed: {error}"));
    let equipment = validate_assemble_equipment(&registries, &state, EQUIPMENT_STONE_PICK, source)
        .unwrap_or_else(|error| panic!("composite pick validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("composite pick commit failed: {error}"));

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("composite pick serialization failed: {error}"));
    encoded["state"]["systems"]["equipment"]["records"][equipment.value().to_string()]["embodied_material"]
        [0]["mass"] = serde_json::json!(200_001_u64);
    let tampered: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("composite pick tamper decode failed: {error}"));

    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Equipment(
            EquipmentValidationError::EmbodiedTraceMassMismatch {
                equipment,
                stored: Mass::from_milligrams(1_000_000),
                traced: Mass::from_milligrams(1_000_001),
            }
        )))
    );
}
