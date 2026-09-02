//! Replayable ordinary-play woodworking investment episode for the cold-agent report.

use deep_hearth::content::gameplay_fixture::seed_lot;
use deep_hearth::content::{
    EQUIPMENT_STONE_WOODWORKING_ADZE, EQUIPMENT_TIMBER_FRAME_SAW_BENCH, FORM_BOARD, FORM_CHIP,
    FORM_LOG, FORM_LUMP, FORM_NATIVE_METAL, MATERIAL_COPPER, MATERIAL_STONE, MATERIAL_WOOD,
    PROCESS_COLD_WORK_COPPER_SAW_BLADE, PROCESS_KNAP_STONE_TOOL, PROCESS_SAW_WOOD_BOARDS,
    PROCESS_SHAPE_WOOD_BOARDS, PROCESS_SHAPE_WOOD_HANDLE,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::core::time::WorldSeed;
use deep_hearth::crafting::resolve_manual_craft;
use deep_hearth::equipment::{EquipmentId, validate_assemble_equipment};
use deep_hearth::inventory::StockpileId;
use deep_hearth::material::CommodityKey;
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::registry::Registries;
use deep_hearth::survival::initialize_player_survival;

use super::environment::ROOM_TEMPERATURE;
use super::focused_runner::focused_probe_role_label;
use super::focused_seeds::FocusedProbeCase;
use super::inventory_support::add_solid_stockpile;
use super::manual_craft_execution::{execute_manual_craft, execute_manual_craft_batches};
use super::manual_craft_selection::select_manual_craft_request;
use super::seed::mix64;

fn assemble_adze(
    registries: &Registries,
    state: &mut AppState,
    raw: StockpileId,
    parts: StockpileId,
) -> (EquipmentId, u64) {
    let edge = execute_manual_craft_batches(
        registries,
        state,
        PROCESS_KNAP_STONE_TOOL,
        raw,
        parts,
        1,
        "woodworking adze edge",
    );
    let handle = execute_manual_craft_batches(
        registries,
        state,
        PROCESS_SHAPE_WOOD_HANDLE,
        raw,
        parts,
        1,
        "woodworking adze handle",
    );
    let equipment =
        validate_assemble_equipment(registries, state, EQUIPMENT_STONE_WOODWORKING_ADZE, parts)
            .unwrap_or_else(|error| panic!("woodworking adze assembly failed: {error}"))
            .commit(state)
            .unwrap_or_else(|error| panic!("woodworking adze assembly commit failed: {error}"));
    (
        equipment,
        edge.value()
            .checked_add(handle.value())
            .unwrap_or_else(|| panic!("woodworking adze setup overflowed")),
    )
}

fn assemble_saw(
    registries: &Registries,
    state: &mut AppState,
    raw: StockpileId,
    parts: StockpileId,
    adze: EquipmentId,
) -> (EquipmentId, u64) {
    let frame = execute_manual_craft(
        registries,
        state,
        select_manual_craft_request(
            registries,
            state,
            PROCESS_SHAPE_WOOD_BOARDS,
            raw,
            3,
            "woodworking saw frame",
        )
        .with_equipment(adze),
        parts,
        "woodworking saw frame",
    );
    let handle = execute_manual_craft_batches(
        registries,
        state,
        PROCESS_SHAPE_WOOD_HANDLE,
        raw,
        parts,
        1,
        "woodworking saw handle",
    );
    let blade = execute_manual_craft_batches(
        registries,
        state,
        PROCESS_COLD_WORK_COPPER_SAW_BLADE,
        raw,
        parts,
        1,
        "woodworking saw blade",
    );
    let equipment =
        validate_assemble_equipment(registries, state, EQUIPMENT_TIMBER_FRAME_SAW_BENCH, parts)
            .unwrap_or_else(|error| panic!("woodworking saw assembly failed: {error}"))
            .commit(state)
            .unwrap_or_else(|error| panic!("woodworking saw assembly commit failed: {error}"));
    let attention = frame
        .value()
        .checked_add(handle.value())
        .and_then(|ticks| ticks.checked_add(blade.value()))
        .unwrap_or_else(|| panic!("woodworking saw setup overflowed"));
    (equipment, attention)
}

pub(super) fn run_woodworking_probe(registries: &Registries, case: FocusedProbeCase) {
    let seed = case.seed();
    let project_batches = 3 + mix64(seed ^ 0x574F_4F44_5052_4F4A) % 10;
    let blade_input = registries
        .crafting()
        .get_manual(PROCESS_COLD_WORK_COPPER_SAW_BLADE)
        .map(|definition| definition.input_mass())
        .unwrap_or_else(|| panic!("woodworking saw-blade process disappeared"));
    let copper_available = if mix64(seed ^ 0x574F_4F44_434F_5050).is_multiple_of(2) {
        blade_input
    } else {
        Mass::from_milligrams(blade_input.milligrams().saturating_sub(1))
    };
    // Bounded actor policy, not an oracle: large visible projects justify investigating the saw,
    // but only when the currently owned copper can actually fund its authored blade route.
    let saw_candidate = project_batches >= 8;
    let choose_saw = saw_candidate && copper_available >= blade_input;

    let mut state = AppState::new(WorldSeed::new(seed ^ 0x574F_4F44_574F_524C));
    let raw = add_solid_stockpile(&mut state, Mass::from_milligrams(25_100_000));
    seed_lot(
        registries,
        &mut state,
        raw,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(1_000_000),
        ROOM_TEMPERATURE,
    );
    seed_lot(
        registries,
        &mut state,
        raw,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(24_000_000),
        ROOM_TEMPERATURE,
    );
    if !copper_available.is_zero() {
        seed_lot(
            registries,
            &mut state,
            raw,
            CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
            copper_available,
            ROOM_TEMPERATURE,
        );
    }
    let adze_parts = add_solid_stockpile(&mut state, Mass::from_milligrams(2_000_000));
    let saw_parts = add_solid_stockpile(&mut state, Mass::from_milligrams(8_000_000));
    let output = add_solid_stockpile(&mut state, Mass::from_milligrams(24_000_000));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("woodworking initial matter audit failed: {error}"))
        .total();
    initialize_player_survival(registries, &mut state)
        .unwrap_or_else(|error| panic!("woodworking survival setup failed: {error}"));

    let (adze, adze_setup) = assemble_adze(registries, &mut state, raw, adze_parts);
    let adze_request = select_manual_craft_request(
        registries,
        &state,
        PROCESS_SHAPE_WOOD_BOARDS,
        raw,
        project_batches,
        "woodworking adze projection",
    )
    .with_equipment(adze);
    let adze_projection = resolve_manual_craft(registries, &state, &adze_request)
        .unwrap_or_else(|error| panic!("woodworking adze projection failed: {error}"));
    let bare_projection = resolve_manual_craft(
        registries,
        &state,
        &select_manual_craft_request(
            registries,
            &state,
            PROCESS_SHAPE_WOOD_BOARDS,
            raw,
            project_batches,
            "woodworking bare projection",
        ),
    )
    .unwrap_or_else(|error| panic!("woodworking bare projection failed: {error}"));
    assert_eq!(
        adze_projection.output_streams(),
        bare_projection.output_streams()
    );
    assert!(adze_projection.duration() < bare_projection.duration());
    let adze_board_mass = adze_projection
        .single_output_stream()
        .unwrap_or_else(|| panic!("woodworking adze projection lost its output stream"))
        .outputs()
        .iter()
        .find(|output| output.commodity() == CommodityKey::new(MATERIAL_WOOD, FORM_BOARD))
        .map(|output| output.mass())
        .unwrap_or_else(|| panic!("woodworking adze projection lost its board output"));

    let (choice, setup_ticks, production_ticks) = if choose_saw {
        let (saw, saw_setup) = assemble_saw(registries, &mut state, raw, saw_parts, adze);
        let request = select_manual_craft_request(
            registries,
            &state,
            PROCESS_SAW_WOOD_BOARDS,
            raw,
            project_batches,
            "woodworking selected saw project",
        )
        .with_equipment(saw);
        let duration = execute_manual_craft(
            registries,
            &mut state,
            request,
            output,
            "woodworking selected saw project",
        );
        ("frame-saw", adze_setup + saw_setup, duration)
    } else {
        let duration = execute_manual_craft(
            registries,
            &mut state,
            adze_request,
            output,
            "woodworking selected adze project",
        );
        ("stone-adze", adze_setup, duration)
    };

    let output = state
        .inventory()
        .get_stockpile(output)
        .unwrap_or_else(|| panic!("woodworking output stockpile disappeared"));
    let boards = output.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_BOARD));
    let chips = output.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_CHIP));
    assert_eq!(
        boards.checked_add(chips),
        Some(Mass::from_milligrams(project_batches * 1_000_000)),
        "woodworking selected path must conserve the project timber"
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("woodworking final matter audit failed: {error}"))
            .total(),
        matter_before
    );
    validate_loaded_state(registries, &state)
        .unwrap_or_else(|error| panic!("woodworking final state invalid: {error}"));

    let reason = if choose_saw {
        "large-project+copper-available"
    } else if saw_candidate {
        "copper-supply-limited"
    } else {
        "project-too-small-for-saw-policy"
    };
    let selected_attention = setup_ticks
        .checked_add(production_ticks.value())
        .unwrap_or_else(|| panic!("woodworking selected attention overflowed"));
    let adze_attention = adze_setup
        .checked_add(adze_projection.duration().value())
        .unwrap_or_else(|| panic!("woodworking adze attention overflowed"));
    let attention_delta = i128::from(selected_attention) - i128::from(adze_attention);
    let board_delta = i128::from(boards.milligrams()) - i128::from(adze_board_mass.milligrams());
    std::println!(
        "WOODWORKING EXPERIENCE seed=0x{seed:016X} sample={} project={}logs copper={}mg choice={choice} reason={reason} setup={}t production={}t selected-total={}t recovery=[boards:{}mg chips:{}mg] adze-counterfactual=[total:{}t boards:{}mg] tradeoff=[attention:{:+}t boards:{:+}mg] baseline=[bare:{}t adze-production:{}t] matter=conserved",
        focused_probe_role_label(case.role()),
        project_batches,
        copper_available.milligrams(),
        setup_ticks,
        production_ticks.value(),
        selected_attention,
        boards.milligrams(),
        chips.milligrams(),
        adze_attention,
        adze_board_mass.milligrams(),
        attention_delta,
        board_delta,
        bare_projection.duration().value(),
        adze_projection.duration().value(),
    );
}
