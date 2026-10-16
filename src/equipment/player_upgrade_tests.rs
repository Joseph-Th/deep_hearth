//! Contracts for actor-facing equipment upgrade discovery and start.

use super::*;
use crate::content::{
    EQUIPMENT_COPPER_REINFORCED_PICK, EQUIPMENT_STONE_PICK, FORM_HANDLE, FORM_TOOL, MATERIAL_STONE,
    MATERIAL_WOOD, build_registries,
};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::AppState;
use crate::equipment::validate_assemble_equipment;
use crate::inventory::deposit_lot_for_test;
use crate::material::CommodityKey;
use crate::matter::calculate_matter_accounting;
use crate::player::validate_admit_player;
use crate::spatial::VoxelCoord;

#[test]
fn player_equipment_upgrade_option_tracks_material_readiness_and_revalidates_base() {
    let registries = build_registries();
    let mut state = AppState::new();
    let carried = validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 0))
        .unwrap_or_else(|error| panic!("upgrade catalog player admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("upgrade catalog player commit failed: {error}"))
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
        .unwrap_or_else(|error| panic!("upgrade catalog pick material setup failed: {error}"));
    }
    let pick = validate_assemble_equipment(&registries, &state, EQUIPMENT_STONE_PICK, carried)
        .unwrap_or_else(|error| panic!("upgrade catalog pick assembly failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("upgrade catalog pick commit failed: {error}"));

    let unavailable = player_equipment_upgrade_options(&registries, &state, carried)
        .unwrap_or_else(|error| panic!("upgrade catalog projection failed: {error}"))
        .into_iter()
        .find(|option| {
            option.equipment() == pick
                && option.target_definition() == EQUIPMENT_COPPER_REINFORCED_PICK
        })
        .unwrap_or_else(|| panic!("reinforced-pick upgrade option disappeared"));
    assert!(!unavailable.material_ready());

    let upgrade = registries
        .equipment()
        .get_equipment(EQUIPMENT_COPPER_REINFORCED_PICK)
        .and_then(|definition| definition.upgrade_profile())
        .unwrap_or_else(|| panic!("reinforced-pick upgrade profile disappeared"));
    for input in upgrade.additions().inputs() {
        deposit_lot_for_test(
            &registries,
            &mut state,
            carried,
            input.commodity(),
            input.mass(),
            Temperature::from_millikelvin(293_150),
        )
        .unwrap_or_else(|error| panic!("upgrade catalog reinforcement setup failed: {error}"));
    }
    let ready = player_equipment_upgrade_options(&registries, &state, carried)
        .unwrap_or_else(|error| panic!("ready upgrade catalog projection failed: {error}"))
        .into_iter()
        .find(|option| {
            option.equipment() == pick
                && option.target_definition() == EQUIPMENT_COPPER_REINFORCED_PICK
        })
        .unwrap_or_else(|| panic!("ready reinforced-pick upgrade option disappeared"));
    assert!(ready.material_ready());
    assert_eq!(ready.base_definition(), EQUIPMENT_STONE_PICK);
    assert_eq!(ready.source(), carried);
    assert_eq!(ready.additions(), upgrade.additions().inputs());
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("upgrade catalog matter setup failed: {error}"))
        .total();

    let upgraded = validate_upgrade_player_equipment_option(&registries, &state, &ready)
        .unwrap_or_else(|error| panic!("upgrade option start failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("upgrade option commit failed: {error}"));
    assert_eq!(upgraded, pick);
    assert_eq!(
        state
            .equipment()
            .get_equipment(pick)
            .map(|record| record.definition()),
        Some(EQUIPMENT_COPPER_REINFORCED_PICK)
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("upgrade catalog final matter audit failed: {error}"))
            .total(),
        matter_before
    );
    assert!(matches!(
        validate_upgrade_player_equipment_option(&registries, &state, &ready),
        Err(EquipmentUpgradeError::WrongBaseDefinition {
            equipment,
            required: EQUIPMENT_STONE_PICK,
            actual: EQUIPMENT_COPPER_REINFORCED_PICK,
        }) if equipment == pick
    ));
}
