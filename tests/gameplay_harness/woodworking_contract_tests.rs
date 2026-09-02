//! Ordinary primitive woodworking investment contracts.

use deep_hearth::content::gameplay_fixture::{seed_lot, seed_stockpile};
use deep_hearth::content::{
    EQUIPMENT_COPPER_REINFORCED_WOODWORKING_ADZE, EQUIPMENT_STONE_WOODWORKING_ADZE, FORM_BOARD,
    FORM_BULK_CRATE_BODY, FORM_LOG, FORM_LUMP, FORM_NATIVE_METAL, MATERIAL_COPPER, MATERIAL_STONE,
    MATERIAL_WOOD, PROCESS_ASSEMBLE_BULK_TIMBER_CRATE, PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
    PROCESS_KNAP_STONE_TOOL, PROCESS_SHAPE_WOOD_BOARDS, PROCESS_SHAPE_WOOD_HANDLE,
    STORAGE_BULK_TIMBER_PROVISIONS_CRATE, build_registries,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::core::time::{TickSpan, WorldSeed};
use deep_hearth::crafting::{
    ManualCraftRequest, ManualCraftStartRequest, resolve_manual_craft, validate_start_manual_craft,
};
use deep_hearth::equipment::{validate_assemble_equipment, validate_upgrade_equipment};
use deep_hearth::inventory::{
    StockpileId, StockpileStorageProfile, validate_build_storage_enclosure,
};
use deep_hearth::maintenance::Condition;
use deep_hearth::material::CommodityKey;
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::production::ProcessId;
use deep_hearth::registry::Registries;
use deep_hearth::survival::initialize_player_survival;

use super::environment::ROOM_TEMPERATURE;
use super::manual_craft_selection::select_manual_craft_request;
use super::production_timing::finish_uninterrupted_production_job;

fn finish_manual_craft(
    registries: &Registries,
    state: &mut AppState,
    request: ManualCraftRequest,
    destination: StockpileId,
    context: &'static str,
) -> TickSpan {
    let resolution = resolve_manual_craft(registries, state, &request)
        .unwrap_or_else(|error| panic!("{context} resolution failed: {error}"));
    let duration = resolution.duration();
    let job = validate_start_manual_craft(
        registries,
        state,
        ManualCraftStartRequest::new(request, destination),
    )
    .unwrap_or_else(|error| panic!("{context} start failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| panic!("{context} commit failed: {error}"));
    finish_uninterrupted_production_job(registries, state, job, duration, context);
    duration
}

fn craft_batches(
    registries: &Registries,
    state: &mut AppState,
    process: ProcessId,
    source: StockpileId,
    destination: StockpileId,
    batches: u64,
    context: &'static str,
) -> TickSpan {
    let request = select_manual_craft_request(registries, state, process, source, batches, context);
    finish_manual_craft(registries, state, request, destination, context)
}

#[test]
fn woodworking_adze_turns_bulk_board_work_into_a_durable_attention_investment() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x574F_4F44_4144_5A45));

    let raw = seed_stockpile(
        &mut state,
        Mass::from_milligrams(9_020_000),
        StockpileStorageProfile::unbounded_solid_only(),
    );
    seed_lot(
        &registries,
        &mut state,
        raw,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(1_000_000),
        ROOM_TEMPERATURE,
    );
    seed_lot(
        &registries,
        &mut state,
        raw,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(8_000_000),
        ROOM_TEMPERATURE,
    );
    seed_lot(
        &registries,
        &mut state,
        raw,
        CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
        Mass::from_milligrams(20_000),
        ROOM_TEMPERATURE,
    );
    let components = seed_stockpile(
        &mut state,
        Mass::from_milligrams(2_000_000),
        StockpileStorageProfile::unbounded_solid_only(),
    );
    let boards = seed_stockpile(
        &mut state,
        Mass::from_milligrams(7_000_000),
        StockpileStorageProfile::unbounded_solid_only(),
    );
    let crate_body = seed_stockpile(
        &mut state,
        Mass::from_milligrams(3_200_000),
        StockpileStorageProfile::unbounded_solid_only(),
    );
    let copperwork = seed_stockpile(
        &mut state,
        Mass::from_milligrams(20_000),
        StockpileStorageProfile::unbounded_solid_only(),
    );
    let provisions = seed_stockpile(
        &mut state,
        Mass::from_milligrams(50_000_000),
        StockpileStorageProfile::unbounded_solid_only(),
    );

    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("woodworking progression survival setup failed: {error}"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("woodworking progression matter setup failed: {error}"))
        .total();

    let knap_ticks = craft_batches(
        &registries,
        &mut state,
        PROCESS_KNAP_STONE_TOOL,
        raw,
        components,
        1,
        "woodworking adze stone edge",
    );
    let handle_ticks = craft_batches(
        &registries,
        &mut state,
        PROCESS_SHAPE_WOOD_HANDLE,
        raw,
        components,
        1,
        "woodworking adze handle",
    );
    assert_eq!(
        (knap_ticks, handle_ticks),
        (TickSpan::new(40), TickSpan::new(40))
    );
    let setup_attention = knap_ticks
        .value()
        .checked_add(handle_ticks.value())
        .unwrap_or_else(|| panic!("woodworking setup attention overflowed"));

    let adze = validate_assemble_equipment(
        &registries,
        &state,
        EQUIPMENT_STONE_WOODWORKING_ADZE,
        components,
    )
    .unwrap_or_else(|error| panic!("ordinary woodworking adze assembly failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("ordinary woodworking adze commit failed: {error}"));

    let bare_bulk_request = select_manual_craft_request(
        &registries,
        &state,
        PROCESS_SHAPE_WOOD_BOARDS,
        raw,
        4,
        "bulk-crate bare board counterfactual",
    );
    let bare_bulk = resolve_manual_craft(&registries, &state, &bare_bulk_request)
        .unwrap_or_else(|error| panic!("bulk-crate bare board counterfactual failed: {error}"));
    let assisted_bulk_request = select_manual_craft_request(
        &registries,
        &state,
        PROCESS_SHAPE_WOOD_BOARDS,
        raw,
        4,
        "bulk-crate adze boards",
    )
    .with_equipment(adze);
    let assisted_bulk = resolve_manual_craft(&registries, &state, &assisted_bulk_request)
        .unwrap_or_else(|error| panic!("bulk-crate adze board resolution failed: {error}"));
    assert_eq!(bare_bulk.duration(), TickSpan::new(200));
    assert_eq!(assisted_bulk.duration(), TickSpan::new(112));
    assert_eq!(assisted_bulk.output_streams(), bare_bulk.output_streams());
    assert!(
        setup_attention + assisted_bulk.duration().value() < bare_bulk.duration().value(),
        "four board batches should repay the base adze's 80-tick preparation attention"
    );

    let three_batch_request = select_manual_craft_request(
        &registries,
        &state,
        PROCESS_SHAPE_WOOD_BOARDS,
        raw,
        3,
        "three-batch adze payback counterfactual",
    )
    .with_equipment(adze);
    let three_batch_adze = resolve_manual_craft(&registries, &state, &three_batch_request)
        .unwrap_or_else(|error| panic!("three-batch adze counterfactual failed: {error}"));
    assert!(
        setup_attention + three_batch_adze.duration().value() >= 150,
        "the stone adze must remain an investment instead of dominating small carpentry jobs"
    );

    let board_ticks = finish_manual_craft(
        &registries,
        &mut state,
        assisted_bulk_request,
        boards,
        "bulk-crate adze boards",
    );
    assert_eq!(board_ticks, TickSpan::new(112));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(boards)
            .map(|stockpile| stockpile.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_BOARD))),
        Some(Mass::from_milligrams(3_200_000))
    );

    let crate_ticks = craft_batches(
        &registries,
        &mut state,
        PROCESS_ASSEMBLE_BULK_TIMBER_CRATE,
        boards,
        crate_body,
        1,
        "bulk provisions crate joinery",
    );
    assert_eq!(crate_ticks, TickSpan::new(90));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(crate_body)
            .map(|stockpile| {
                stockpile.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_BULK_CRATE_BODY))
            }),
        Some(Mass::from_milligrams(3_200_000))
    );
    validate_build_storage_enclosure(
        &registries,
        &state,
        STORAGE_BULK_TIMBER_PROVISIONS_CRATE,
        provisions,
        crate_body,
    )
    .unwrap_or_else(|error| panic!("bulk provisions crate construction failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("bulk provisions crate construction commit failed: {error}"));
    assert_eq!(
        state
            .inventory()
            .get_stockpile(provisions)
            .and_then(|stockpile| stockpile.enclosure())
            .map(|enclosure| enclosure.definition()),
        Some(STORAGE_BULK_TIMBER_PROVISIONS_CRATE)
    );

    let stone_future_request = select_manual_craft_request(
        &registries,
        &state,
        PROCESS_SHAPE_WOOD_BOARDS,
        raw,
        3,
        "worn stone adze future project",
    )
    .with_equipment(adze);
    let stone_future = resolve_manual_craft(&registries, &state, &stone_future_request)
        .unwrap_or_else(|error| panic!("worn stone adze future project failed: {error}"));
    let condition_before_upgrade = state
        .equipment()
        .get_equipment(adze)
        .map(|record| record.condition())
        .unwrap_or_else(|| panic!("woodworking adze disappeared before reinforcement"));
    assert_eq!(
        condition_before_upgrade,
        Condition::new(888_000).unwrap_or_else(|error| panic!("condition failed: {error}"))
    );

    let reinforcement_ticks = craft_batches(
        &registries,
        &mut state,
        PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
        raw,
        copperwork,
        1,
        "woodworking copper reinforcement",
    );
    assert_eq!(reinforcement_ticks, TickSpan::new(40));
    let upgraded = validate_upgrade_equipment(
        &registries,
        &state,
        adze,
        EQUIPMENT_COPPER_REINFORCED_WOODWORKING_ADZE,
        copperwork,
    )
    .unwrap_or_else(|error| panic!("woodworking adze reinforcement failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("woodworking adze reinforcement commit failed: {error}"));
    assert_eq!(upgraded, adze);
    assert_eq!(
        state
            .equipment()
            .get_equipment(adze)
            .map(|record| (record.definition(), record.condition())),
        Some((
            EQUIPMENT_COPPER_REINFORCED_WOODWORKING_ADZE,
            condition_before_upgrade,
        )),
        "woodworking reinforcement must preserve equipment identity and accumulated wear"
    );

    let copper_future_request = select_manual_craft_request(
        &registries,
        &state,
        PROCESS_SHAPE_WOOD_BOARDS,
        raw,
        3,
        "reinforced adze future project",
    )
    .with_equipment(adze);
    let copper_future = resolve_manual_craft(&registries, &state, &copper_future_request)
        .unwrap_or_else(|error| panic!("reinforced adze future project failed: {error}"));
    let future_savings = stone_future
        .duration()
        .value()
        .checked_sub(copper_future.duration().value())
        .unwrap_or_else(|| panic!("reinforced woodworking unexpectedly increased attention"));
    assert!(
        future_savings >= reinforcement_ticks.value(),
        "three future board batches should repay the 40-tick copper-reinforcement shaping cost"
    );
    assert_eq!(
        copper_future.output_streams(),
        stone_future.output_streams()
    );
    let reinforced_ticks = finish_manual_craft(
        &registries,
        &mut state,
        copper_future_request,
        boards,
        "reinforced adze future project",
    );
    assert_eq!(reinforced_ticks, copper_future.duration());

    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("woodworking progression matter audit failed: {error}"))
            .total(),
        matter_before
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("woodworking progression final state invalid: {error}"));
}
