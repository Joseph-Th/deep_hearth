//! Replayable ordinary-play woodworking investment episode for the cold-agent report.

use deep_hearth::content::gameplay_fixture::seed_lot;
use deep_hearth::content::{
    EQUIPMENT_STONE_WOODWORKING_ADZE, EQUIPMENT_TIMBER_FRAME_SAW_BENCH, FORM_BOARD, FORM_CHIP,
    FORM_LOG, FORM_LUMP, FORM_NATIVE_METAL, MATERIAL_COPPER, MATERIAL_STONE, MATERIAL_WOOD,
    PROCESS_COLD_WORK_COPPER_SAW_BLADE, PROCESS_SAW_WOOD_BOARDS, PROCESS_SHAPE_WOOD_BOARDS,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::core::time::WorldSeed;
use deep_hearth::crafting::resolve_manual_craft;
use deep_hearth::equipment::{EquipmentId, validate_assemble_equipment};
use deep_hearth::inventory::StockpileId;
use deep_hearth::material::CommodityKey;
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::production::ProcessResolution;
use deep_hearth::registry::Registries;
use deep_hearth::survival::initialize_player_survival;

use super::environment::ROOM_TEMPERATURE;
use super::focused_runner::focused_probe_role_label;
use super::focused_seeds::FocusedProbeCase;
use super::inventory_support::add_solid_stockpile;
use super::manual_craft_execution::{execute_manual_craft, execute_manual_craft_batches};
use super::manual_craft_planning::manual_craft_plan_for_output;
use super::manual_craft_selection::select_manual_craft_request;
use super::seed::mix64;

fn assemble_adze(
    registries: &Registries,
    state: &mut AppState,
    raw: StockpileId,
    parts: StockpileId,
) -> (EquipmentId, u64) {
    let assembly = registries
        .equipment()
        .get_equipment(EQUIPMENT_STONE_WOODWORKING_ADZE)
        .and_then(|definition| definition.assembly_profile())
        .unwrap_or_else(|| panic!("woodworking adze lost its authored assembly"));
    let mut attention = 0_u64;
    for input in assembly.inputs() {
        let (craft, batches) = manual_craft_plan_for_output(
            registries,
            input.commodity(),
            input.mass(),
            "woodworking adze component planning",
        );
        let duration = execute_manual_craft_batches(
            registries,
            state,
            craft.process(),
            raw,
            parts,
            batches,
            "woodworking adze component",
        );
        attention = attention
            .checked_add(duration.value())
            .unwrap_or_else(|| panic!("woodworking adze setup overflowed"));
    }
    let equipment =
        validate_assemble_equipment(registries, state, EQUIPMENT_STONE_WOODWORKING_ADZE, parts)
            .unwrap_or_else(|error| panic!("woodworking adze assembly failed: {error}"))
            .commit(state)
            .unwrap_or_else(|error| panic!("woodworking adze assembly commit failed: {error}"));
    (equipment, attention)
}

fn assemble_saw(
    registries: &Registries,
    state: &mut AppState,
    raw: StockpileId,
    parts: StockpileId,
    adze: EquipmentId,
) -> (EquipmentId, u64) {
    let assembly = registries
        .equipment()
        .get_equipment(EQUIPMENT_TIMBER_FRAME_SAW_BENCH)
        .and_then(|definition| definition.assembly_profile())
        .unwrap_or_else(|| panic!("woodworking frame saw lost its authored assembly"));
    let mut attention = 0_u64;
    for input in assembly.inputs() {
        let duration = if input.commodity() == CommodityKey::new(MATERIAL_WOOD, FORM_BOARD) {
            let board_craft = registries
                .crafting()
                .get_manual(PROCESS_SHAPE_WOOD_BOARDS)
                .unwrap_or_else(|| panic!("woodworking saw-frame board route disappeared"));
            let boards_per_batch = authored_output_mass(board_craft, input.commodity());
            let batches = input
                .mass()
                .milligrams()
                .div_ceil(boards_per_batch.milligrams());
            execute_manual_craft(
                registries,
                state,
                select_manual_craft_request(
                    registries,
                    state,
                    board_craft.process(),
                    raw,
                    batches,
                    "woodworking saw frame",
                )
                .with_equipment(adze),
                parts,
                "woodworking saw frame",
            )
        } else {
            let (craft, batches) = manual_craft_plan_for_output(
                registries,
                input.commodity(),
                input.mass(),
                "woodworking saw component planning",
            );
            execute_manual_craft_batches(
                registries,
                state,
                craft.process(),
                raw,
                parts,
                batches,
                "woodworking saw component",
            )
        };
        attention = attention
            .checked_add(duration.value())
            .unwrap_or_else(|| panic!("woodworking saw setup overflowed"));
    }
    let equipment =
        validate_assemble_equipment(registries, state, EQUIPMENT_TIMBER_FRAME_SAW_BENCH, parts)
            .unwrap_or_else(|error| panic!("woodworking saw assembly failed: {error}"))
            .commit(state)
            .unwrap_or_else(|error| panic!("woodworking saw assembly commit failed: {error}"));
    (equipment, attention)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WoodworkingInvestmentPreference {
    ConserveScarceCopper,
    ConserveTimber,
}

impl WoodworkingInvestmentPreference {
    const fn from_behavior_seed(seed: u64) -> Self {
        if seed.is_multiple_of(2) {
            Self::ConserveScarceCopper
        } else {
            Self::ConserveTimber
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::ConserveScarceCopper => "conserve-scarce-copper",
            Self::ConserveTimber => "conserve-timber",
        }
    }
}

fn authored_output_mass(
    definition: &deep_hearth::crafting::ManualCraftDefinition,
    commodity: CommodityKey,
) -> Mass {
    definition
        .outputs()
        .iter()
        .find(|output| output.commodity() == commodity)
        .map(|output| output.mass())
        .unwrap_or_else(|| {
            panic!(
                "woodworking process {} lost authored output {}",
                definition.process().value(),
                commodity.value()
            )
        })
}

fn projected_board_mass(resolution: &ProcessResolution) -> Mass {
    resolution
        .single_output_stream()
        .unwrap_or_else(|| panic!("woodworking projection lost its single physical output stream"))
        .outputs()
        .iter()
        .find(|output| output.commodity() == CommodityKey::new(MATERIAL_WOOD, FORM_BOARD))
        .map(|output| output.mass())
        .unwrap_or_else(|| panic!("woodworking projection lost its board output"))
}

pub(super) fn run_woodworking_probe(registries: &Registries, case: FocusedProbeCase) {
    let seed = case.seed();
    let behavior_seed = case
        .behavior_seed()
        .unwrap_or_else(|| panic!("woodworking actor case lost its independent behavior seed"));
    let preference = WoodworkingInvestmentPreference::from_behavior_seed(behavior_seed);
    let adze_board_definition = registries
        .crafting()
        .get_manual(PROCESS_SHAPE_WOOD_BOARDS)
        .unwrap_or_else(|| panic!("woodworking adze board process disappeared"));
    let saw_board_definition = registries
        .crafting()
        .get_manual(PROCESS_SAW_WOOD_BOARDS)
        .unwrap_or_else(|| panic!("woodworking saw board process disappeared"));
    let board_commodity = CommodityKey::new(MATERIAL_WOOD, FORM_BOARD);
    let adze_board_mass_per_batch = authored_output_mass(adze_board_definition, board_commodity);
    let saw_board_mass_per_batch = authored_output_mass(saw_board_definition, board_commodity);
    let demand_scale = 3 + mix64(seed ^ 0x574F_4F44_5052_4F4A) % 10;
    let board_demand = Mass::from_milligrams(
        adze_board_mass_per_batch
            .milligrams()
            .checked_mul(demand_scale)
            .unwrap_or_else(|| panic!("woodworking board demand overflowed")),
    );
    let adze_batches = board_demand
        .milligrams()
        .div_ceil(adze_board_mass_per_batch.milligrams());
    let saw_batches = board_demand
        .milligrams()
        .div_ceil(saw_board_mass_per_batch.milligrams());
    assert_eq!(adze_batches, demand_scale);
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
    let saw_fundable = copper_available >= blade_input;

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
        adze_batches,
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
            adze_batches,
            "woodworking bare projection",
        ),
    )
    .unwrap_or_else(|error| panic!("woodworking bare projection failed: {error}"));
    assert_eq!(
        adze_projection.output_streams(),
        bare_projection.output_streams()
    );
    assert!(adze_projection.duration() < bare_projection.duration());
    let adze_board_mass = projected_board_mass(&adze_projection);
    assert_eq!(adze_board_mass, board_demand);
    let adze_project_input = adze_projection.input_mass();

    let saw_saves_timber = saw_batches < adze_batches;
    let invest_in_saw = saw_fundable
        && saw_saves_timber
        && preference == WoodworkingInvestmentPreference::ConserveTimber;
    let (choice, reason, setup_ticks, production_ticks, selected_project_mass) = if invest_in_saw {
        let (saw, saw_setup) = assemble_saw(registries, &mut state, raw, saw_parts, adze);
        let saw_request = select_manual_craft_request(
            registries,
            &state,
            PROCESS_SAW_WOOD_BOARDS,
            raw,
            saw_batches,
            "woodworking saw investment projection",
        )
        .with_equipment(saw);
        let saw_projection = resolve_manual_craft(registries, &state, &saw_request)
            .unwrap_or_else(|error| panic!("woodworking saw projection failed: {error}"));
        let saw_boards = projected_board_mass(&saw_projection);
        assert!(saw_boards >= board_demand);
        assert!(saw_projection.input_mass() < adze_project_input);
        let setup_ticks = adze_setup
            .checked_add(saw_setup)
            .unwrap_or_else(|| panic!("woodworking investigated setup overflowed"));
        let input_mass = saw_projection.input_mass();
        let duration = execute_manual_craft(
            registries,
            &mut state,
            saw_request,
            output,
            "woodworking selected saw project",
        );
        (
            "frame-saw",
            "timber-demand-justifies-saw",
            setup_ticks,
            duration,
            input_mass,
        )
    } else {
        let input_mass = adze_projection.input_mass();
        let duration = execute_manual_craft(
            registries,
            &mut state,
            adze_request,
            output,
            "woodworking selected adze project",
        );
        let reason = if !saw_fundable {
            "copper-supply-limited"
        } else if !saw_saves_timber {
            "current-board-demand-no-timber-savings"
        } else {
            "policy-conserves-scarce-copper"
        };
        ("stone-adze", reason, adze_setup, duration, input_mass)
    };

    let output = state
        .inventory()
        .get_stockpile(output)
        .unwrap_or_else(|| panic!("woodworking output stockpile disappeared"));
    let boards = output.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_BOARD));
    let chips = output.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_CHIP));
    assert_eq!(
        boards.checked_add(chips),
        Some(selected_project_mass),
        "woodworking selected path must conserve the project timber"
    );
    assert!(
        boards >= board_demand,
        "woodworking selected path must satisfy the player-visible board demand"
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("woodworking final matter audit failed: {error}"))
            .total(),
        matter_before
    );
    validate_loaded_state(registries, &state)
        .unwrap_or_else(|error| panic!("woodworking final state invalid: {error}"));

    let selected_attention = setup_ticks
        .checked_add(production_ticks.value())
        .unwrap_or_else(|| panic!("woodworking selected attention overflowed"));
    let adze_attention = adze_setup
        .checked_add(adze_projection.duration().value())
        .unwrap_or_else(|| panic!("woodworking adze attention overflowed"));
    let attention_delta = i128::from(selected_attention) - i128::from(adze_attention);
    let timber_delta = i128::from(selected_project_mass.milligrams())
        - i128::from(adze_project_input.milligrams());
    let board_surplus = boards
        .checked_sub(board_demand)
        .unwrap_or_else(|| unreachable!("selected woodworking path satisfies board demand"));
    reviewln!(
        "WOODWORKING EXPERIENCE seed=0x{seed:016X} behavior=0x{behavior_seed:016X} sample={} demand={}mg boards preference={} copper={}mg routes=[adze:{}logs saw:{}logs saw-fundable:{saw_fundable} saw-saves-timber:{saw_saves_timber}] choice={choice} reason={reason} setup={}t production={}t selected-total={}t selected=[timber:{}mg boards:{}mg surplus:{}mg chips:{}mg] adze-counterfactual=[timber:{}mg total:{}t boards:{}mg] tradeoff=[attention:{:+}t timber:{:+}mg] baseline=[bare:{}t adze-production:{}t] matter=conserved",
        focused_probe_role_label(case.role()),
        board_demand.milligrams(),
        preference.label(),
        copper_available.milligrams(),
        adze_batches,
        saw_batches,
        setup_ticks,
        production_ticks.value(),
        selected_attention,
        selected_project_mass.milligrams(),
        boards.milligrams(),
        board_surplus.milligrams(),
        chips.milligrams(),
        adze_project_input.milligrams(),
        adze_attention,
        adze_board_mass.milligrams(),
        attention_delta,
        timber_delta,
        bare_projection.duration().value(),
        adze_projection.duration().value(),
    );
}
