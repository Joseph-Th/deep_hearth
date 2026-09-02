//! Settlement-scale frame-saw acquisition and timber-efficiency contracts.

use deep_hearth::content::gameplay_fixture::{seed_lot, seed_stockpile};
use deep_hearth::content::{
    EQUIPMENT_STONE_WOODWORKING_ADZE, EQUIPMENT_TIMBER_FRAME_SAW_BENCH, FORM_BOARD, FORM_CHIP,
    FORM_HANDLE, FORM_LOG, FORM_LUMP, FORM_NATIVE_METAL, FORM_SAW_BLADE, FORM_SCRAP,
    MATERIAL_COPPER, MATERIAL_STONE, MATERIAL_WOOD, PROCESS_COLD_WORK_COPPER_SAW_BLADE,
    PROCESS_KNAP_STONE_TOOL, PROCESS_SAW_WOOD_BOARDS, PROCESS_SHAPE_WOOD_BOARDS,
    PROCESS_SHAPE_WOOD_HANDLE, build_registries,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::core::time::{TickSpan, WorldSeed};
use deep_hearth::crafting::{
    ManualCraftError, ManualCraftRequest, ManualCraftStartRequest, resolve_manual_craft,
    validate_start_manual_craft,
};
use deep_hearth::equipment::validate_assemble_equipment;
use deep_hearth::inventory::{StockpileId, StockpileStorageProfile};
use deep_hearth::maintenance::Condition;
use deep_hearth::material::CommodityKey;
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use deep_hearth::production::ProcessId;
use deep_hearth::registry::Registries;
use deep_hearth::simulation::advance_tick;
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
fn frame_saw_bench_turns_scarce_copper_into_better_timber_recovery_and_attention() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5341_575F_4245_4E43));
    let raw = seed_stockpile(
        &mut state,
        Mass::from_milligrams(20_000_000),
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
        Mass::from_milligrams(18_000_000),
        ROOM_TEMPERATURE,
    );
    seed_lot(
        &registries,
        &mut state,
        raw,
        CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
        Mass::from_milligrams(60_000),
        ROOM_TEMPERATURE,
    );
    let adze_parts = seed_stockpile(
        &mut state,
        Mass::from_milligrams(2_000_000),
        StockpileStorageProfile::unbounded_solid_only(),
    );
    let bench_parts = seed_stockpile(
        &mut state,
        Mass::from_milligrams(8_000_000),
        StockpileStorageProfile::unbounded_solid_only(),
    );
    let output = seed_stockpile(
        &mut state,
        Mass::from_milligrams(12_000_000),
        StockpileStorageProfile::unbounded_solid_only(),
    );
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("frame-saw survival setup failed: {error}"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("frame-saw initial matter audit failed: {error}"))
        .total();

    assert_eq!(
        craft_batches(
            &registries,
            &mut state,
            PROCESS_KNAP_STONE_TOOL,
            raw,
            adze_parts,
            1,
            "frame-saw prerequisite stone edge",
        ),
        TickSpan::new(40)
    );
    assert_eq!(
        craft_batches(
            &registries,
            &mut state,
            PROCESS_SHAPE_WOOD_HANDLE,
            raw,
            adze_parts,
            1,
            "frame-saw prerequisite adze handle",
        ),
        TickSpan::new(40)
    );
    let adze = validate_assemble_equipment(
        &registries,
        &state,
        EQUIPMENT_STONE_WOODWORKING_ADZE,
        adze_parts,
    )
    .unwrap_or_else(|error| panic!("frame-saw prerequisite adze assembly failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("frame-saw prerequisite adze commit failed: {error}"));

    let frame_board_request = select_manual_craft_request(
        &registries,
        &state,
        PROCESS_SHAPE_WOOD_BOARDS,
        raw,
        3,
        "frame-saw timber frame boards",
    )
    .with_equipment(adze);
    let frame_board_ticks = finish_manual_craft(
        &registries,
        &mut state,
        frame_board_request,
        bench_parts,
        "frame-saw timber frame boards",
    );
    assert_eq!(frame_board_ticks, TickSpan::new(84));
    let frame_handle_ticks = craft_batches(
        &registries,
        &mut state,
        PROCESS_SHAPE_WOOD_HANDLE,
        raw,
        bench_parts,
        1,
        "frame-saw tension frame handle",
    );
    assert_eq!(frame_handle_ticks, TickSpan::new(40));
    let blade_ticks = craft_batches(
        &registries,
        &mut state,
        PROCESS_COLD_WORK_COPPER_SAW_BLADE,
        raw,
        bench_parts,
        1,
        "frame-saw copper blade",
    );
    assert_eq!(blade_ticks, TickSpan::new(120));
    let bench_stock = state
        .inventory()
        .get_stockpile(bench_parts)
        .unwrap_or_else(|| panic!("frame-saw parts stockpile disappeared"));
    assert_eq!(
        bench_stock.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_BOARD)),
        Mass::from_milligrams(2_400_000)
    );
    assert_eq!(
        bench_stock.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE)),
        Mass::from_milligrams(200_000)
    );
    assert_eq!(
        bench_stock.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_SAW_BLADE)),
        Mass::from_milligrams(54_000)
    );
    assert_eq!(
        bench_stock.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_SCRAP)),
        Mass::from_milligrams(6_000),
        "tooth cutting must leave explicit reusable copper offcut"
    );
    let saw = validate_assemble_equipment(
        &registries,
        &state,
        EQUIPMENT_TIMBER_FRAME_SAW_BENCH,
        bench_parts,
    )
    .unwrap_or_else(|error| panic!("timber frame saw bench assembly failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("timber frame saw bench commit failed: {error}"));

    let one_log = select_manual_craft_request(
        &registries,
        &state,
        PROCESS_SAW_WOOD_BOARDS,
        raw,
        1,
        "frame-saw required-tool rejection",
    );
    assert_eq!(
        resolve_manual_craft(&registries, &state, &one_log).err(),
        Some(ManualCraftError::RequiredEquipmentMissing {
            process: PROCESS_SAW_WOOD_BOARDS,
        })
    );
    assert!(matches!(
        resolve_manual_craft(&registries, &state, &one_log.clone().with_equipment(adze)),
        Err(ManualCraftError::MissingEquipmentCapability { equipment, .. }) if equipment == adze
    ));

    let adze_future = resolve_manual_craft(
        &registries,
        &state,
        &select_manual_craft_request(
            &registries,
            &state,
            PROCESS_SHAPE_WOOD_BOARDS,
            raw,
            12,
            "frame-saw adze counterfactual",
        )
        .with_equipment(adze),
    )
    .unwrap_or_else(|error| panic!("frame-saw adze counterfactual failed: {error}"));
    let saw_request = select_manual_craft_request(
        &registries,
        &state,
        PROCESS_SAW_WOOD_BOARDS,
        raw,
        12,
        "frame-saw production run",
    )
    .with_equipment(saw);
    let saw_future = resolve_manual_craft(&registries, &state, &saw_request)
        .unwrap_or_else(|error| panic!("frame-saw production resolution failed: {error}"));
    assert_eq!(saw_future.duration(), TickSpan::new(84));
    let setup_attention = frame_board_ticks
        .value()
        .checked_add(frame_handle_ticks.value())
        .and_then(|ticks| ticks.checked_add(blade_ticks.value()))
        .unwrap_or_else(|| panic!("frame-saw setup attention overflowed"));
    assert!(
        adze_future.duration().value() - saw_future.duration().value() > setup_attention,
        "twelve future logs should repay the bench-specific 244-tick setup attention"
    );
    let saw_outputs = saw_future
        .single_output_stream()
        .unwrap_or_else(|| panic!("frame-saw output stream disappeared"))
        .outputs();
    assert!(saw_outputs.iter().any(|lot| {
        lot.commodity() == CommodityKey::new(MATERIAL_WOOD, FORM_BOARD)
            && lot.mass() == Mass::from_milligrams(10_800_000)
    }));
    assert!(saw_outputs.iter().any(|lot| {
        lot.commodity() == CommodityKey::new(MATERIAL_WOOD, FORM_CHIP)
            && lot.mass() == Mass::from_milligrams(1_200_000)
    }));
    let adze_outputs = adze_future
        .single_output_stream()
        .unwrap_or_else(|| panic!("adze counterfactual output stream disappeared"))
        .outputs();
    assert!(adze_outputs.iter().any(|lot| {
        lot.commodity() == CommodityKey::new(MATERIAL_WOOD, FORM_BOARD)
            && lot.mass() == Mass::from_milligrams(9_600_000)
    }));

    let job = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::new(saw_request, output),
    )
    .unwrap_or_else(|error| panic!("frame-saw production start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("frame-saw production commit failed: {error}"));
    for _ in 0..20 {
        let outcome = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("frame-saw pre-save tick failed: {error}"));
        assert!(outcome.production_completions().is_empty());
    }
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("frame-saw save encoding failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("frame-saw save decoding failed: {error}"));
    let mut loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("frame-saw trusted load failed: {error}"));
    assert_eq!(loaded, state);
    while state.production().get_job(job).is_some() {
        let expected = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("frame-saw continuation failed: {error}"));
        let actual = advance_tick(&registries, &mut loaded)
            .unwrap_or_else(|error| panic!("frame-saw loaded continuation failed: {error}"));
        assert_eq!(actual, expected);
    }
    assert_eq!(loaded, state);
    assert_eq!(
        state
            .equipment()
            .get_equipment(saw)
            .map(|record| record.condition()),
        Some(Condition::new(874_000).unwrap_or_else(|error| panic!("condition failed: {error}")))
    );
    let output_stock = state
        .inventory()
        .get_stockpile(output)
        .unwrap_or_else(|| panic!("frame-saw output stockpile disappeared"));
    assert_eq!(
        output_stock.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_BOARD)),
        Mass::from_milligrams(10_800_000)
    );
    assert_eq!(
        output_stock.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_CHIP)),
        Mass::from_milligrams(1_200_000)
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("frame-saw final matter audit failed: {error}"))
            .total(),
        matter_before
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("frame-saw final state invalid: {error}"));
}
