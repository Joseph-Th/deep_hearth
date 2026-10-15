//! Contracts for ordinary recipe/batch crafting from player-carried custody.

use std::num::NonZeroU64;

use super::*;
use crate::content::{
    FORM_CHIP, FORM_LOG, FORM_LUMP, FORM_TOOL, MATERIAL_STONE, MATERIAL_WOOD,
    PROCESS_KNAP_STONE_TOOL, PROCESS_SHAPE_WOOD_BOARDS, build_registries,
};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::AppState;
use crate::inventory::{deposit_lot_for_test, project_material_stacks};
use crate::material::CommodityKey;
use crate::matter::calculate_matter_accounting;
use crate::player::validate_admit_player;
use crate::simulation::advance_tick;
use crate::spatial::VoxelCoord;

fn one_batch() -> NonZeroU64 {
    NonZeroU64::new(1).unwrap_or_else(|| unreachable!("one batch is nonzero"))
}

#[test]
fn personal_recipe_catalog_reads_only_carried_material_availability() {
    let registries = build_registries();
    let mut state = AppState::new();
    let carried = validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 0))
        .unwrap_or_else(|error| panic!("personal recipe player admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("personal recipe player commit failed: {error}"))
        .carried_stockpile();
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(2_000_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("personal recipe carried stone failed: {error}"));

    let options = player_manual_craft_options(&registries, &state)
        .unwrap_or_else(|error| panic!("personal recipe catalog failed: {error}"));
    let stone = options
        .iter()
        .copied()
        .find(|option| option.process() == PROCESS_KNAP_STONE_TOOL)
        .unwrap_or_else(|| panic!("personal stone recipe disappeared"));
    assert!(matches!(
        stone.input_mode(),
        crate::crafting::ManualCraftInputMode::Automatic(availability)
            if availability.maximum_batches() == 2
    ));
    let boards = options
        .iter()
        .copied()
        .find(|option| option.process() == PROCESS_SHAPE_WOOD_BOARDS)
        .unwrap_or_else(|| panic!("personal board recipe disappeared"));
    assert!(matches!(
        boards.input_mode(),
        crate::crafting::ManualCraftInputMode::Automatic(availability)
            if availability.maximum_batches() == 0
    ));
}

#[test]
fn personal_recipe_batch_crafts_in_place_without_player_lot_identity() {
    let registries = build_registries();
    let mut state = AppState::new();
    let carried = validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 0))
        .unwrap_or_else(|error| panic!("personal craft player admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("personal craft player commit failed: {error}"))
        .carried_stockpile();
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(1_000_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("personal craft carried stone failed: {error}"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("personal craft initial matter audit failed: {error}"))
        .total();

    let job = validate_start_player_manual_craft(
        &registries,
        &state,
        PROCESS_KNAP_STONE_TOOL,
        one_batch(),
        None,
    )
    .unwrap_or_else(|error| panic!("personal recipe/batch start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("personal recipe/batch commit failed: {error}"));
    let completes_at = state
        .production()
        .get_job(job)
        .unwrap_or_else(|| panic!("personal craft job disappeared"))
        .completes_at();
    while state.tick() < completes_at {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("personal craft tick failed: {error}"));
    }

    let record = state
        .inventory()
        .get_stockpile(carried)
        .unwrap_or_else(|| panic!("personal craft carried stockpile disappeared"));
    assert_eq!(
        record.get_mass(CommodityKey::new(MATERIAL_STONE, FORM_TOOL)),
        Mass::from_milligrams(800_000)
    );
    assert_eq!(
        record.get_mass(CommodityKey::new(MATERIAL_STONE, FORM_CHIP)),
        Mass::from_milligrams(200_000)
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("personal craft final matter audit failed: {error}"))
            .total(),
        matter_before
    );
}

#[test]
fn explicit_personal_stack_choice_is_revision_bound_and_must_match_recipe_input() {
    let registries = build_registries();
    let mut state = AppState::new();
    let carried = validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 0))
        .unwrap_or_else(|error| panic!("explicit personal craft admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("explicit personal craft commit failed: {error}"))
        .carried_stockpile();
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(1_000_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("explicit personal stone failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(1_000_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("explicit personal wood failed: {error}"));
    let stacks = project_material_stacks(&registries, &state, carried)
        .unwrap_or_else(|error| panic!("explicit personal stack projection failed: {error}"));
    let stone = stacks
        .iter()
        .find(|stack| stack.commodity() == CommodityKey::new(MATERIAL_STONE, FORM_LUMP))
        .unwrap_or_else(|| panic!("explicit personal stone stack disappeared"));
    let wood = stacks
        .iter()
        .find(|stack| stack.commodity() == CommodityKey::new(MATERIAL_WOOD, FORM_LOG))
        .unwrap_or_else(|| panic!("explicit personal wood stack disappeared"));

    assert_eq!(
        validate_start_player_manual_craft_from_stack(
            &registries,
            &state,
            PROCESS_KNAP_STONE_TOOL,
            one_batch(),
            wood,
            None,
        )
        .err(),
        Some(PlayerManualCraftError::StackCommodityMismatch {
            expected: CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
            actual: CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        })
    );

    let stale_stone = stone.clone();
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(100_000),
        Temperature::from_millikelvin(303_150),
    )
    .unwrap_or_else(|error| panic!("explicit personal stale mutation failed: {error}"));
    assert!(matches!(
        validate_start_player_manual_craft_from_stack(
            &registries,
            &state,
            PROCESS_KNAP_STONE_TOOL,
            one_batch(),
            &stale_stone,
            None,
        ),
        Err(PlayerManualCraftError::Stack(
            MaterialStackProjectionError::StaleInventoryRevision { .. }
        ))
    ));
}
