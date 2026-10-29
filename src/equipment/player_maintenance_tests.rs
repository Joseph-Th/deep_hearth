//! Contracts for player-facing equipment service discovery and carried-stock admission.

use super::*;
use crate::content::{
    EQUIPMENT_STONE_PICK, FORM_HANDLE, FORM_TOOL, MATERIAL_STONE, MATERIAL_WOOD, build_registries,
};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::{AppState, validate_loaded_state};
use crate::equipment::{degrade_equipment_condition_for_test, validate_assemble_equipment};
use crate::inventory::{add_solid_stockpile_for_test, deposit_lot_for_test};
use crate::logistics::validate_place_ground_stockpile;
use crate::maintenance::Condition;
use crate::material::CommodityKey;
use crate::matter::calculate_matter_accounting;
use crate::player::validate_admit_player;
use crate::simulation::advance_tick;
use crate::spatial::VoxelCoord;

fn worn_pick_with_player() -> (Registries, AppState, EquipmentId) {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let assembly = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000))
        .unwrap_or_else(|error| panic!("player maintenance assembly stock failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        assembly,
        CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
        Mass::from_milligrams(800_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("player maintenance stone input failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        assembly,
        CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
        Mass::from_milligrams(200_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("player maintenance handle input failed: {error}"));
    validate_place_ground_stockpile(&state, assembly, position)
        .unwrap_or_else(|error| panic!("player maintenance assembly placement failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("player maintenance assembly placement commit failed: {error}")
        });
    validate_admit_player(&registries, &state, position)
        .unwrap_or_else(|error| panic!("player maintenance admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("player maintenance admission commit failed: {error}"));
    let pick = validate_assemble_equipment(&registries, &state, EQUIPMENT_STONE_PICK, assembly)
        .unwrap_or_else(|error| panic!("player maintenance pick assembly failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("player maintenance pick commit failed: {error}"));
    degrade_equipment_condition_for_test(&mut state, pick, 100_000);
    (registries, state, pick)
}

#[test]
fn player_maintenance_catalog_exposes_material_need_before_and_after_carried_stock_exists() {
    let (registries, mut state, pick) = worn_pick_with_player();
    let option = player_equipment_maintenance_options(&registries, &state)
        .unwrap_or_else(|error| panic!("player maintenance catalog failed: {error}"))
        .into_iter()
        .find(|option| option.equipment() == pick)
        .unwrap_or_else(|| panic!("worn local pick disappeared from player maintenance catalog"));
    assert_eq!(option.definition(), EQUIPMENT_STONE_PICK);
    assert!(!option.replacement_ready());
    assert!(option.required_replacement_mass() > Mass::ZERO);
    assert!(option.target_condition() > option.condition_before());
    assert!(option.duration().value() > 0);

    let carried = state
        .logistics()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("player maintenance player disappeared"))
        .carried_stockpile();
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        option.replacement(),
        option.required_replacement_mass(),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("player maintenance carried replacement failed: {error}"));
    let ready = player_equipment_maintenance_options(&registries, &state)
        .unwrap_or_else(|error| panic!("ready player maintenance catalog failed: {error}"))
        .into_iter()
        .find(|candidate| candidate.equipment() == pick)
        .unwrap_or_else(|| panic!("ready pick disappeared from maintenance catalog"));
    assert!(ready.replacement_ready());
}

#[test]
fn player_maintenance_action_services_local_pick_and_returns_spent_material_to_carried_custody() {
    let (registries, mut state, pick) = worn_pick_with_player();
    let option = player_equipment_maintenance_options(&registries, &state)
        .unwrap_or_else(|error| panic!("player maintenance catalog failed: {error}"))
        .into_iter()
        .find(|option| option.equipment() == pick)
        .unwrap_or_else(|| panic!("worn pick disappeared before service"));
    let carried = state
        .logistics()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("player maintenance player disappeared before service"))
        .carried_stockpile();
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        option.replacement(),
        option.required_replacement_mass(),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("player maintenance replacement stock failed: {error}"));

    let start = validate_start_player_equipment_maintenance(&registries, &state, pick)
        .unwrap_or_else(|error| panic!("player maintenance start failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("player maintenance commit failed: {error}"));
    assert_eq!(start.equipment(), pick);
    assert_eq!(start.target_condition(), option.target_condition());
    assert_eq!(start.material_mass(), option.required_replacement_mass());
    assert_eq!(
        state
            .inventory()
            .get_stockpile(carried)
            .map(|stockpile| stockpile.get_mass(option.spent())),
        Some(option.required_replacement_mass()),
        "service must return the replaced material to carried custody as its authored spent form"
    );
    while state.tick() < start.completes_at() {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("player maintenance completion tick failed: {error}"));
    }
    assert_eq!(
        state
            .equipment()
            .get_equipment(pick)
            .map(|record| record.condition()),
        Some(option.target_condition())
    );
    assert!(
        player_equipment_maintenance_options(&registries, &state)
            .unwrap_or_else(|error| panic!("post-service maintenance catalog failed: {error}"))
            .into_iter()
            .all(|candidate| candidate.equipment() != pick),
        "fully serviced pick should leave the current maintenance-needs catalog"
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    assert_eq!(
        state
            .equipment()
            .get_equipment(pick)
            .map(|record| record.condition()),
        Some(Condition::PRISTINE)
    );
}

#[test]
fn hot_replacement_is_not_ready_and_maintenance_rejects_without_mutation() {
    let (registries, mut state, pick) = worn_pick_with_player();
    let carried = state.logistics().player().unwrap().carried_stockpile();
    let option = player_equipment_maintenance_options(&registries, &state)
        .unwrap()
        .into_iter()
        .find(|option| option.equipment() == pick)
        .unwrap();
    let hot = Temperature::from_millikelvin(400_000);
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        option.replacement(),
        option.required_replacement_mass(),
        hot,
    )
    .unwrap();
    let before = state.clone();
    assert!(
        !player_equipment_maintenance_options(&registries, &state)
            .unwrap()
            .into_iter()
            .find(|candidate| candidate.equipment() == pick)
            .unwrap()
            .replacement_ready()
    );
    assert!(matches!(
        validate_start_player_equipment_maintenance(&registries, &state, pick),
        Err(PlayerEquipmentMaintenanceStartError::Resolution(
            EquipmentMaintenanceResolutionError::ReplacementMaterialTooHot {
                commodity,
                temperature,
                maximum,
            }
        )) if commodity == option.replacement()
            && temperature == hot && maximum == Temperature::from_millikelvin(353_150)
    ));
    assert_eq!(state, before);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn maintenance_uses_newer_cool_replacement_without_consuming_hot_material() {
    let (registries, mut state, pick) = worn_pick_with_player();
    let carried = state.logistics().player().unwrap().carried_stockpile();
    let option = player_equipment_maintenance_options(&registries, &state)
        .unwrap()
        .into_iter()
        .find(|option| option.equipment() == pick)
        .unwrap();
    let hot_lot = deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        option.replacement(),
        option.required_replacement_mass(),
        Temperature::from_millikelvin(400_000),
    )
    .unwrap();
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        option.replacement(),
        option.required_replacement_mass(),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    let matter_before = calculate_matter_accounting(&state).unwrap().total();
    assert!(
        player_equipment_maintenance_options(&registries, &state)
            .unwrap()
            .into_iter()
            .find(|candidate| candidate.equipment() == pick)
            .unwrap()
            .replacement_ready()
    );
    let started = validate_start_player_equipment_maintenance(&registries, &state, pick)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(started.equipment(), pick);
    assert_eq!(started.material_mass(), option.required_replacement_mass());
    assert_eq!(
        state.inventory().get_lot(hot_lot).unwrap().mass(),
        option.required_replacement_mass()
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        matter_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
