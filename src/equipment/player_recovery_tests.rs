//! Contracts for ordinary equipment recovery into player-carried custody.

use super::*;
use crate::content::{
    EQUIPMENT_STONE_PICK, FORM_HANDLE, FORM_SCRAP, FORM_TOOL, MATERIAL_STONE, MATERIAL_WOOD,
    build_registries,
};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::{AppState, validate_loaded_state};
use crate::equipment::{degrade_equipment_condition_for_test, validate_assemble_equipment};
use crate::inventory::{add_solid_stockpile_for_test, deposit_lot_for_test};
use crate::logistics::validate_place_ground_stockpile;
use crate::material::CommodityKey;
use crate::matter::calculate_matter_accounting;
use crate::player::validate_admit_player;
use crate::spatial::VoxelCoord;

#[test]
fn player_disassembly_recovers_worn_local_pick_into_carried_custody_without_wear_reset() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let assembly = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000))
        .unwrap_or_else(|error| panic!("player recovery assembly stock failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        assembly,
        CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
        Mass::from_milligrams(800_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("player recovery stone input failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        assembly,
        CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
        Mass::from_milligrams(200_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("player recovery handle input failed: {error}"));
    validate_place_ground_stockpile(&state, assembly, position)
        .unwrap_or_else(|error| panic!("player recovery source placement failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("player recovery source placement commit failed: {error}"));
    let carried = validate_admit_player(&registries, &state, position)
        .unwrap_or_else(|error| panic!("player recovery admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("player recovery admission commit failed: {error}"))
        .carried_stockpile();
    let pick = validate_assemble_equipment(&registries, &state, EQUIPMENT_STONE_PICK, assembly)
        .unwrap_or_else(|error| panic!("player recovery pick assembly failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("player recovery pick assembly commit failed: {error}"));
    degrade_equipment_condition_for_test(&mut state, pick, 1);
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("player recovery matter baseline failed: {error}"))
        .total();

    let outcome = validate_player_disassemble_equipment(&registries, &state, pick)
        .unwrap_or_else(|error| panic!("player recovery validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("player recovery commit failed: {error}"));

    assert_eq!(outcome.recovered_lots().len(), 2);
    assert!(state.equipment().get_equipment(pick).is_none());
    assert_eq!(state.logistics().stationary_equipment_position(pick), None);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|stockpile| stockpile.get_mass(CommodityKey::new(MATERIAL_STONE, FORM_SCRAP))),
        Some(Mass::from_milligrams(800_000))
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|stockpile| stockpile.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE))),
        Some(Mass::from_milligrams(200_000))
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|stockpile| stockpile.get_mass(CommodityKey::new(MATERIAL_STONE, FORM_TOOL))),
        Some(Mass::ZERO),
        "worn working stone must not reappear as pristine reusable tool stock"
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("player recovery matter audit failed: {error}"))
            .total(),
        matter_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn player_disassembly_requires_an_admitted_player_to_choose_carried_recovery() {
    let registries = build_registries();
    let state = AppState::new();
    assert!(matches!(
        validate_player_disassemble_equipment(&registries, &state, EquipmentId::new(1)),
        Err(PlayerEquipmentRecoveryError::PlayerNotAdmitted)
    ));
}
