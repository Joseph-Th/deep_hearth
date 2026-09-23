//! End-to-end manual-crafting coverage for loss-bearing timber scrap recovery.

use super::{ManualCraftStartRequest, validate_start_manual_craft};
use crate::content::{
    EQUIPMENT_TIMBER_RIDDLE_SIZING_SCREEN, FORM_BOARD, FORM_CHIP, FORM_HANDLE, FORM_SCRAP,
    FORM_TIMBER_RIDDLE_PANEL, MATERIAL_WOOD, PROCESS_RECOVER_WOOD_SCRAP_BOARDS,
    PROCESS_REWORK_WOOD_SCRAP_HANDLE, build_registries,
};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::{AppState, validate_loaded_state};
use crate::equipment::{
    EquipmentMaintenanceRequest, degrade_equipment_condition_for_test,
    resolve_equipment_maintenance, validate_assemble_equipment, validate_equipment_maintenance,
};
use crate::inventory::{MaterialLotSelection, add_solid_stockpile_for_test, deposit_lot_for_test};
use crate::maintenance::Condition;
use crate::material::CommodityKey;
use crate::matter::calculate_matter_accounting;
use crate::simulation::advance_tick;
use crate::survival::initialize_player_survival;

#[test]
fn maintained_timber_scrap_flows_into_lossy_handle_and_board_recovery() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("wood scrap recovery survival setup failed: {error}"));
    let assembly = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_600_000))
        .unwrap_or_else(|error| {
            panic!("wood scrap recovery riddle assembly stock failed: {error}")
        });
    deposit_lot_for_test(
        &registries,
        &mut state,
        assembly,
        CommodityKey::new(MATERIAL_WOOD, FORM_TIMBER_RIDDLE_PANEL),
        Mass::from_milligrams(1_400_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("wood scrap recovery riddle panel failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        assembly,
        CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
        Mass::from_milligrams(200_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("wood scrap recovery riddle handle failed: {error}"));
    let riddle = validate_assemble_equipment(
        &registries,
        &state,
        EQUIPMENT_TIMBER_RIDDLE_SIZING_SCREEN,
        assembly,
    )
    .unwrap_or_else(|error| panic!("wood scrap recovery riddle assembly failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("wood scrap recovery riddle assembly commit failed: {error}"));
    degrade_equipment_condition_for_test(&mut state, riddle, 1_000_000);
    assert_eq!(
        state
            .equipment()
            .get_equipment(riddle)
            .map(|record| record.condition()),
        Some(Condition::FAILED)
    );
    let replacement = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_400_000))
        .unwrap_or_else(|error| panic!("wood scrap recovery replacement stock failed: {error}"));
    let spent = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_400_000))
        .unwrap_or_else(|error| panic!("wood scrap recovery spent stock failed: {error}"));
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_250_000))
        .unwrap_or_else(|error| panic!("wood scrap recovery destination failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        replacement,
        CommodityKey::new(MATERIAL_WOOD, FORM_TIMBER_RIDDLE_PANEL),
        Mass::from_milligrams(1_400_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("wood scrap recovery replacement panel failed: {error}"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("wood scrap recovery initial matter audit failed: {error}"))
        .total();

    let maintenance = resolve_equipment_maintenance(
        &registries,
        &state,
        EquipmentMaintenanceRequest::new(riddle, replacement, spent),
    )
    .unwrap_or_else(|error| panic!("wood scrap recovery maintenance resolution failed: {error}"));
    assert_eq!(
        maintenance.material_mass(),
        Mass::from_milligrams(1_400_000)
    );
    assert_eq!(
        maintenance.spent_commodity(),
        CommodityKey::new(MATERIAL_WOOD, FORM_SCRAP)
    );
    let maintenance_start = validate_equipment_maintenance(&registries, &state, maintenance)
        .unwrap_or_else(|error| {
            panic!("wood scrap recovery maintenance validation failed: {error}")
        })
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("wood scrap recovery maintenance commit failed: {error}"));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(spent)
            .map(|record| record.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_SCRAP))),
        Some(Mass::from_milligrams(1_400_000)),
        "riddle service must emit its worn timber panel as the exact scrap commodity recovery consumes"
    );
    while state.tick() < maintenance_start.completes_at() {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("wood scrap recovery maintenance tick failed: {error}"));
    }
    assert_eq!(
        state
            .equipment()
            .get_equipment(riddle)
            .map(|record| record.condition()),
        Some(Condition::PRISTINE)
    );

    let board_scrap =
        state.inventory().lot_ids(spent).next().unwrap_or_else(|| {
            panic!("maintained riddle produced no recoverable timber scrap lot")
        });

    validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(
            PROCESS_RECOVER_WOOD_SCRAP_BOARDS,
            spent,
            MaterialLotSelection::new(board_scrap, Mass::from_milligrams(1_000_000)),
            destination,
        ),
    )
    .unwrap_or_else(|error| panic!("wood scrap board recovery failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("wood scrap board recovery commit failed: {error}"));
    for _ in 0..60 {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("wood scrap board recovery tick failed: {error}"));
    }

    let handle_scrap = state
        .inventory()
        .lot_ids(spent)
        .next()
        .unwrap_or_else(|| panic!("board recovery left no scrap for handle recovery"));

    validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(
            PROCESS_REWORK_WOOD_SCRAP_HANDLE,
            spent,
            MaterialLotSelection::new(handle_scrap, Mass::from_milligrams(250_000)),
            destination,
        ),
    )
    .unwrap_or_else(|error| panic!("wood scrap handle recovery failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("wood scrap handle recovery commit failed: {error}"));
    for _ in 0..30 {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("wood scrap handle recovery tick failed: {error}"));
    }

    let recovered = state
        .inventory()
        .get_stockpile(destination)
        .unwrap_or_else(|| panic!("wood scrap recovery destination disappeared"));
    assert_eq!(
        recovered.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_BOARD)),
        Mass::from_milligrams(600_000)
    );
    assert_eq!(
        recovered.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE)),
        Mass::from_milligrams(200_000)
    );
    assert_eq!(
        recovered.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_CHIP)),
        Mass::from_milligrams(450_000)
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(spent)
            .map(|record| record.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_SCRAP))),
        Some(Mass::from_milligrams(150_000)),
        "recovery must leave the unselected maintenance scrap in custody rather than silently consuming it"
    );
    let useful_recovered = recovered
        .get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_BOARD))
        .checked_add(recovered.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE)))
        .unwrap_or_else(|| panic!("wood scrap useful-form recovery mass overflowed"));
    assert!(
        useful_recovered < Mass::from_milligrams(1_400_000),
        "scrap recovery must lose useful board/handle form even while conserving timber matter"
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!(
                "wood scrap recovery final matter audit failed: {error}"
            ))
            .total(),
        matter_before
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("wood scrap recovery final state invalid: {error}"));
}
