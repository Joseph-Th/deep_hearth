//! Contracts for actor-facing equipment assembly discovery and start.

use super::*;
use crate::content::{
    EQUIPMENT_STONE_PICK, FORM_HANDLE, FORM_TOOL, MATERIAL_STONE, MATERIAL_WOOD, build_registries,
};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::AppState;
use crate::inventory::deposit_lot_for_test;
use crate::material::CommodityKey;
use crate::matter::calculate_matter_accounting;
use crate::player::validate_admit_player;
use crate::spatial::VoxelCoord;

#[test]
fn player_equipment_assembly_option_revalidates_current_carried_material() {
    let registries = build_registries();
    let mut state = AppState::new();
    let carried = validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 0))
        .unwrap_or_else(|error| panic!("equipment catalog player admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("equipment catalog player commit failed: {error}"))
        .carried_stockpile();
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
            carried,
            commodity,
            mass,
            Temperature::from_millikelvin(293_150),
        )
        .unwrap_or_else(|error| panic!("equipment catalog material setup failed: {error}"));
    }
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("equipment catalog initial matter audit failed: {error}"))
        .total();

    let option = player_equipment_assembly_options(&registries, &state, carried)
        .unwrap_or_else(|error| panic!("equipment assembly catalog failed: {error}"))
        .into_iter()
        .find(|option| option.definition() == EQUIPMENT_STONE_PICK)
        .unwrap_or_else(|| panic!("stone pick assembly option disappeared"));
    assert!(option.is_ready());
    assert_eq!(option.source(), carried);
    assert_eq!(option.embodied_mass(), Mass::from_milligrams(1_000_000));
    assert_eq!(option.inputs().len(), 2);

    let equipment = validate_assemble_player_equipment_option(&registries, &state, &option)
        .unwrap_or_else(|error| panic!("equipment option start failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("equipment option commit failed: {error}"));
    assert_eq!(
        state
            .equipment()
            .get_equipment(equipment)
            .map(|record| record.definition()),
        Some(EQUIPMENT_STONE_PICK)
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("equipment catalog final matter audit failed: {error}"))
            .total(),
        matter_before
    );
    let depleted = player_equipment_assembly_options(&registries, &state, carried)
        .unwrap_or_else(|error| panic!("depleted equipment assembly catalog failed: {error}"))
        .into_iter()
        .find(|candidate| candidate.definition() == EQUIPMENT_STONE_PICK)
        .unwrap_or_else(|| panic!("depleted stone pick assembly option disappeared"));
    assert!(matches!(
        depleted.availability(),
        PlayerEquipmentAssemblyAvailability::MissingMaterial { available, .. }
            if available.is_zero()
    ));
    assert!(matches!(
        validate_assemble_player_equipment_option(&registries, &state, &option),
        Err(EquipmentAssemblyError::InsufficientMaterial {
            stockpile,
            available,
            ..
        }) if stockpile == carried && available.is_zero()
    ));
}
