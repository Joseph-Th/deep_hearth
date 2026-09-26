//! Familiar recipe-input planning over exact inventory lots.

use std::num::NonZeroU64;

use super::*;
use crate::content::{
    FORM_BOARD, FORM_FOOD, FORM_LUMP, MATERIAL_BERRIES, MATERIAL_STONE, MATERIAL_WOOD,
    PROCESS_ASSEMBLE_ROUGH_TIMBER_FIELD_BOX, PROCESS_KNAP_STONE_TOOL, PROCESS_SAW_WOOD_BOARDS,
    PROCESS_SHAPE_WOOD_BOARDS, build_registries,
};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::{AppState, StateValidationError};
use crate::inventory::{add_solid_stockpile_for_test, deposit_lot_for_test};
use crate::labor::PlayerWorkValidationError;
use crate::logistics::{
    GroundStockpilePlacementCommitError, PlayerStockpileAccessError,
    validate_allocate_ground_stockpile, validate_initialize_player_logistics,
    validate_place_ground_stockpile,
};
use crate::material::CommodityKey;
use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
use crate::registry::ProcessEquipmentRole;
use crate::simulation::advance_tick;
use crate::spatial::VoxelCoord;
use crate::survival::initialize_player_survival;

fn batches(value: u64) -> NonZeroU64 {
    NonZeroU64::new(value)
        .unwrap_or_else(|| panic!("manual-craft test batch count must be nonzero"))
}

#[test]
fn manual_craft_rejects_known_remote_ground_source() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("remote craft survival setup failed: {error}"));
    let player_position = VoxelCoord::new(0, 0, 0);
    let carried = validate_initialize_player_logistics(
        &state,
        player_position,
        Mass::from_milligrams(2_000_000),
    )
    .unwrap_or_else(|error| panic!("remote craft logistics setup failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("remote craft logistics commit failed: {error}"))
    .carried_stockpile();
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000))
        .unwrap_or_else(|error| panic!("remote craft source failed: {error}"));
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, crate::content::FORM_LOG),
        Mass::from_milligrams(1_000_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("remote craft lot failed: {error}"));
    let source_position = VoxelCoord::new(1, 0, 0);
    validate_place_ground_stockpile(&state, source, source_position)
        .unwrap_or_else(|error| panic!("remote craft placement failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("remote craft placement commit failed: {error}"));
    let before = state.clone();

    assert_eq!(
        validate_start_manual_craft(
            &registries,
            &state,
            ManualCraftStartRequest::single(
                PROCESS_SHAPE_WOOD_BOARDS,
                source,
                crate::inventory::MaterialLotSelection::new(lot, Mass::from_milligrams(1_000_000)),
                carried,
            ),
        )
        .err(),
        Some(StartManualCraftError::Access(
            PlayerStockpileAccessError::RemoteKnownStockpile {
                stockpile: source,
                stockpile_position: source_position,
                player_position,
            }
        ))
    );
    assert_eq!(state, before);
}

#[test]
fn trusted_load_rejects_active_manual_craft_with_remote_output_destination() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("remote-load craft survival setup failed: {error}"));
    let player_position = VoxelCoord::new(0, 0, 0);
    let carried = validate_initialize_player_logistics(
        &state,
        player_position,
        Mass::from_milligrams(1_000_000),
    )
    .unwrap_or_else(|error| panic!("remote-load craft logistics setup failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("remote-load craft logistics commit failed: {error}"))
    .carried_stockpile();
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(1_000_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("remote-load craft input failed: {error}"));
    let destination = validate_allocate_ground_stockpile(
        &state,
        player_position,
        Mass::from_milligrams(1_000_000),
    )
    .unwrap_or_else(|error| panic!("remote-load craft destination allocation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| {
        panic!("remote-load craft destination allocation commit failed: {error}")
    });
    let _ = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(
            PROCESS_KNAP_STONE_TOOL,
            carried,
            MaterialLotSelection::new(lot, Mass::from_milligrams(1_000_000)),
            destination,
        ),
    )
    .unwrap_or_else(|error| panic!("remote-load craft start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("remote-load craft start commit failed: {error}"));
    crate::core::state::validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("valid remote-load craft fixture failed: {error}"));

    let remote_position = VoxelCoord::new(1, 0, 0);
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("remote-load craft serialization failed: {error}"));
    encoded["state"]["systems"]["logistics"]["ground_stockpiles"]
        [destination.value().to_string()] = serde_json::json!({"x": 1, "y": 0, "z": 0});
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("remote-load craft decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::PlayerWork(
            PlayerWorkValidationError::ManualProductionAccess(
                PlayerStockpileAccessError::RemoteKnownStockpile {
                    stockpile: destination,
                    stockpile_position: remote_position,
                    player_position,
                }
            )
        )))
    );
}

#[test]
fn manual_craft_token_rejects_location_added_after_validation() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("stale-location craft survival setup failed: {error}"));
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000))
        .unwrap_or_else(|error| panic!("stale-location craft source failed: {error}"));
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, crate::content::FORM_LOG),
        Mass::from_milligrams(1_000_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("stale-location craft lot failed: {error}"));
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000))
        .unwrap_or_else(|error| panic!("stale-location craft destination failed: {error}"));
    let validated = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(
            PROCESS_SHAPE_WOOD_BOARDS,
            source,
            crate::inventory::MaterialLotSelection::new(lot, Mass::from_milligrams(1_000_000)),
            destination,
        ),
    )
    .unwrap_or_else(|error| panic!("stale-location craft validation failed: {error}"));
    validate_place_ground_stockpile(&state, source, VoxelCoord::new(5, 0, 0))
        .unwrap_or_else(|error| panic!("stale-location craft placement failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("stale-location craft placement commit failed: {error}"));
    let before = state.clone();

    assert_eq!(
        validated.commit(&mut state),
        Err(ManualCraftCommitError::StaleLogisticsRevision {
            expected: 0,
            actual: 1,
        })
    );
    assert_eq!(state, before);
}

#[test]
fn stockpile_recipe_catalog_exposes_material_counts_and_tool_roles_in_stable_order() {
    let registries = build_registries();
    let mut state = AppState::new();
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(4_000_000))
        .unwrap_or_else(|error| panic!("recipe-catalog stockpile failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
        Mass::from_milligrams(1_600_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("recipe-catalog boards failed: {error}"));

    let options = manual_craft_options_from_stockpile(&registries, &state, source)
        .unwrap_or_else(|error| panic!("recipe catalog failed: {error}"));
    assert!(
        options
            .windows(2)
            .all(|pair| pair[0].process() < pair[1].process()),
        "recipe catalog must retain stable process-id order"
    );

    let rough_box = options
        .iter()
        .copied()
        .find(|option| option.process() == PROCESS_ASSEMBLE_ROUGH_TIMBER_FIELD_BOX)
        .unwrap_or_else(|| panic!("rough field box disappeared from recipe catalog"));
    assert_eq!(rough_box.equipment_role(), ProcessEquipmentRole::None);
    assert!(matches!(
        rough_box.input_mode(),
        ManualCraftInputMode::Automatic(availability)
            if availability.maximum_batches() == 1
    ));

    let hewing = options
        .iter()
        .copied()
        .find(|option| option.process() == PROCESS_SHAPE_WOOD_BOARDS)
        .unwrap_or_else(|| panic!("board hewing disappeared from recipe catalog"));
    assert_eq!(hewing.equipment_role(), ProcessEquipmentRole::Optional);
    assert!(matches!(
        hewing.input_mode(),
        ManualCraftInputMode::Automatic(availability)
            if availability.maximum_batches() == 0
    ));

    let sawing = options
        .iter()
        .copied()
        .find(|option| option.process() == PROCESS_SAW_WOOD_BOARDS)
        .unwrap_or_else(|| panic!("board sawing disappeared from recipe catalog"));
    assert_eq!(sawing.equipment_role(), ProcessEquipmentRole::Required);
}

#[test]
fn familiar_recipe_planner_selects_exact_lots_without_exposing_lot_choice() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("manual-craft planner survival setup failed: {error}"));
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(3_000_000))
        .unwrap_or_else(|error| panic!("manual-craft planner stockpile failed: {error}"));
    let first = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(1_200_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("manual-craft planner first lot failed: {error}"));
    let second = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(1_000_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("manual-craft planner second lot failed: {error}"));
    assert_eq!(
        second, first,
        "compatible non-perishable ingress should present as one physical stack/lot"
    );

    let availability =
        assess_manual_craft_inputs(&registries, &state, PROCESS_KNAP_STONE_TOOL, source)
            .unwrap_or_else(|error| panic!("manual-craft input availability failed: {error}"));
    assert_eq!(availability.maximum_batches(), 2);
    assert_eq!(
        availability.total_eligible_mass(),
        Mass::from_milligrams(2_200_000)
    );

    let request = plan_manual_craft_from_stockpile(
        &registries,
        &state,
        PROCESS_KNAP_STONE_TOOL,
        source,
        batches(2),
    )
    .unwrap_or_else(|error| panic!("manual-craft input planning failed: {error}"));
    assert_eq!(
        request.selections(),
        &[MaterialLotSelection::new(
            first,
            Mass::from_milligrams(2_000_000),
        )]
    );
    let resolution = resolve_manual_craft(&registries, &state, &request)
        .unwrap_or_else(|error| panic!("planned manual craft did not resolve: {error}"));
    assert_eq!(resolution.duration(), crate::core::time::TickSpan::new(80));
}

#[test]
fn familiar_recipe_planner_does_not_hide_temperature_incompatibility() {
    let registries = build_registries();
    let mut state = AppState::new();
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2_000_000))
        .unwrap_or_else(|error| panic!("split-temperature stockpile failed: {error}"));
    for temperature in [293_150, 303_150] {
        deposit_lot_for_test(
            &registries,
            &mut state,
            source,
            CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
            Mass::from_milligrams(600_000),
            Temperature::from_millikelvin(temperature),
        )
        .unwrap_or_else(|error| panic!("split-temperature lot failed: {error}"));
    }

    let availability =
        assess_manual_craft_inputs(&registries, &state, PROCESS_KNAP_STONE_TOOL, source)
            .unwrap_or_else(|error| panic!("split-temperature availability failed: {error}"));
    assert_eq!(
        availability.total_eligible_mass(),
        Mass::from_milligrams(1_200_000)
    );
    assert_eq!(
        availability.largest_compatible_mass(),
        Mass::from_milligrams(600_000)
    );
    assert_eq!(availability.maximum_batches(), 0);
    assert_eq!(availability.craftable_temperature_groups(), 0);
    assert_eq!(
        plan_manual_craft_from_stockpile(
            &registries,
            &state,
            PROCESS_KNAP_STONE_TOOL,
            source,
            batches(1),
        ),
        Err(ManualCraftInputPlanError::SplitTemperatureInput {
            input: CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
            available: Mass::from_milligrams(1_200_000),
            largest_compatible: Mass::from_milligrams(600_000),
            required: Mass::from_milligrams(1_000_000),
        })
    );
}

#[test]
fn familiar_recipe_planner_requires_choice_when_two_temperatures_can_each_supply_the_batch() {
    let registries = build_registries();
    let mut state = AppState::new();
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2_000_000))
        .unwrap_or_else(|error| panic!("temperature-choice stockpile failed: {error}"));
    for temperature in [303_150, 293_150] {
        deposit_lot_for_test(
            &registries,
            &mut state,
            source,
            CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
            Mass::from_milligrams(1_000_000),
            Temperature::from_millikelvin(temperature),
        )
        .unwrap_or_else(|error| panic!("temperature-choice lot failed: {error}"));
    }

    let availability =
        assess_manual_craft_inputs(&registries, &state, PROCESS_KNAP_STONE_TOOL, source)
            .unwrap_or_else(|error| panic!("temperature-choice availability failed: {error}"));
    assert_eq!(availability.maximum_batches(), 1);
    assert_eq!(availability.craftable_temperature_groups(), 2);

    let options = manual_craft_options_from_stockpile(&registries, &state, source)
        .unwrap_or_else(|error| panic!("temperature-choice catalog failed: {error}"));
    let knapping = options
        .iter()
        .copied()
        .find(|option| option.process() == PROCESS_KNAP_STONE_TOOL)
        .unwrap_or_else(|| panic!("stone knapping disappeared from recipe catalog"));
    assert!(matches!(
        knapping.input_mode(),
        ManualCraftInputMode::TemperatureChoice(found) if found == availability
    ));

    assert_eq!(
        plan_manual_craft_from_stockpile(
            &registries,
            &state,
            PROCESS_KNAP_STONE_TOOL,
            source,
            batches(1),
        ),
        Err(
            ManualCraftInputPlanError::MultipleCompatibleInputTemperatures {
                input: CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
                required: Mass::from_milligrams(1_000_000),
                temperatures: vec![
                    Temperature::from_millikelvin(293_150),
                    Temperature::from_millikelvin(303_150),
                ],
            }
        )
    );
}

#[test]
fn in_place_crafting_reuses_released_capacity_in_the_familiar_inventory_path() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("in-place craft survival setup failed: {error}"));
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000))
        .unwrap_or_else(|error| panic!("in-place craft stockpile failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(1_000_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("in-place craft stone failed: {error}"));
    let craft = plan_manual_craft_from_stockpile(
        &registries,
        &state,
        PROCESS_KNAP_STONE_TOOL,
        source,
        batches(1),
    )
    .unwrap_or_else(|error| panic!("in-place craft planning failed: {error}"));
    let request = ManualCraftStartRequest::in_place(craft);
    assert_eq!(request.destination(), source);

    let started = validate_start_manual_craft(&registries, &state, request)
        .unwrap_or_else(|error| {
            panic!("full inventory should reuse outgoing craft capacity: {error}")
        })
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("in-place craft start commit failed: {error}"));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(source)
            .map(|record| (record.stored_mass(), record.reserved_inbound(),)),
        Some((Mass::ZERO, Mass::from_milligrams(1_000_000)))
    );

    let completes_at = state
        .production()
        .get_job(started)
        .unwrap_or_else(|| panic!("in-place craft job disappeared after admission"))
        .completes_at();
    while state.tick() < completes_at {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("in-place craft advancement failed: {error}"));
    }
    assert_eq!(
        state
            .inventory()
            .get_stockpile(source)
            .map(|record| (record.stored_mass(), record.reserved_inbound(),)),
        Some((Mass::from_milligrams(1_000_000), Mass::ZERO))
    );
}

#[test]
fn ground_placement_token_rejects_craft_output_reservation_added_after_validation() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("placement-race craft survival setup failed: {error}"));
    let stockpile = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000))
        .unwrap_or_else(|error| panic!("placement-race craft stockpile failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(1_000_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("placement-race craft stone failed: {error}"));
    let placement = validate_place_ground_stockpile(&state, stockpile, VoxelCoord::new(2, 0, 0))
        .unwrap_or_else(|error| panic!("placement-race ground validation failed: {error}"));
    let craft = plan_manual_craft_from_stockpile(
        &registries,
        &state,
        PROCESS_KNAP_STONE_TOOL,
        stockpile,
        batches(1),
    )
    .unwrap_or_else(|error| panic!("placement-race craft planning failed: {error}"));
    let _ = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::in_place(craft),
    )
    .unwrap_or_else(|error| panic!("placement-race craft start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("placement-race craft commit failed: {error}"));
    let reserved = state
        .inventory()
        .get_stockpile(stockpile)
        .map(|record| record.reserved_inbound())
        .unwrap_or_else(|| panic!("placement-race craft stockpile disappeared"));
    assert_eq!(reserved, Mass::from_milligrams(1_000_000));
    let before = state.clone();

    assert_eq!(
        placement.commit(&mut state),
        Err(GroundStockpilePlacementCommitError::ReservedInbound {
            stockpile,
            reserved,
        })
    );
    assert_eq!(state, before);
}

#[test]
fn familiar_recipe_planner_keeps_perishable_stack_choice_explicit() {
    let registries = build_registries();
    assert!(
        super::input_planning::input_requires_explicit_stack_choice(
            &registries,
            CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
        ),
        "perishable food should keep freshness cohort selection explicit"
    );
    assert!(
        !super::input_planning::input_requires_explicit_stack_choice(
            &registries,
            CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        ),
        "ordinary durable craft inputs should be safe for deterministic auto-selection"
    );
}
