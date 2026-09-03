//! Replayable ordinary-play woodworking investment episode for the cold-agent report.

use deep_hearth::content::gameplay_fixture::seed_lot;
use deep_hearth::content::{
    EQUIPMENT_STONE_WOODWORKING_ADZE, EQUIPMENT_TIMBER_FRAME_SAW_BENCH, FORM_BOARD, FORM_CHIP,
    FORM_LOG, FORM_LUMP, FORM_NATIVE_METAL, MATERIAL_COPPER, MATERIAL_STONE, MATERIAL_WOOD,
    PROCESS_COLD_WORK_COPPER_SAW_BLADE, PROCESS_KNAP_STONE_TOOL, PROCESS_SAW_WOOD_BOARDS,
    PROCESS_SHAPE_WOOD_BOARDS,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::core::time::WorldSeed;
use deep_hearth::crafting::resolve_manual_craft;
use deep_hearth::equipment::{
    EquipmentId, EquipmentMaintenanceRequest, resolve_equipment_maintenance,
    validate_assemble_equipment, validate_equipment_maintenance,
};
use deep_hearth::inventory::StockpileId;
use deep_hearth::maintenance::MaintenanceBand;
use deep_hearth::material::CommodityKey;
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::production::ProcessResolution;
use deep_hearth::registry::Registries;
use deep_hearth::survival::initialize_player_survival;

use super::environment::ROOM_TEMPERATURE;
use super::focused_runner::focused_probe_role_label;
use super::focused_seeds::FocusedProbeCase;
use super::inventory_support::add_solid_stockpile;
use super::maintenance_timing::finish_active_equipment_maintenance;
use super::manual_craft_execution::{execute_manual_craft, execute_manual_craft_batches};
use super::manual_craft_planning::manual_craft_plan_for_output;
use super::manual_craft_selection::select_manual_craft_request;
use super::physical_time::format_physical_duration;
use super::seed::mix64;

fn signed_physical_duration(registries: &Registries, ticks: i128) -> String {
    let magnitude = u64::try_from(ticks.unsigned_abs())
        .unwrap_or_else(|_| panic!("woodworking signed duration exceeds u64"));
    format!(
        "{}{}",
        if ticks < 0 { "-" } else { "+" },
        format_physical_duration(registries, magnitude)
    )
}

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SawSetup {
    equipment: EquipmentId,
    attention_ticks: u64,
    raw_timber: Mass,
}

fn checked_mass_times(mass: Mass, count: u64, context: &'static str) -> Mass {
    Mass::from_milligrams(
        mass.milligrams()
            .checked_mul(count)
            .unwrap_or_else(|| panic!("woodworking {context} mass overflowed")),
    )
}

fn assemble_saw(
    registries: &Registries,
    state: &mut AppState,
    raw: StockpileId,
    parts: StockpileId,
    adze: EquipmentId,
) -> SawSetup {
    let assembly = registries
        .equipment()
        .get_equipment(EQUIPMENT_TIMBER_FRAME_SAW_BENCH)
        .and_then(|definition| definition.assembly_profile())
        .unwrap_or_else(|| panic!("woodworking frame saw lost its authored assembly"));
    let mut attention = 0_u64;
    let mut raw_timber = Mass::ZERO;
    for input in assembly.inputs() {
        let (duration, input_timber) =
            if input.commodity() == CommodityKey::new(MATERIAL_WOOD, FORM_BOARD) {
                let board_craft = registries
                    .crafting()
                    .get_manual(PROCESS_SHAPE_WOOD_BOARDS)
                    .unwrap_or_else(|| panic!("woodworking saw-frame board route disappeared"));
                let boards_per_batch = authored_output_mass(board_craft, input.commodity());
                let batches = input
                    .mass()
                    .milligrams()
                    .div_ceil(boards_per_batch.milligrams());
                (
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
                    ),
                    checked_mass_times(board_craft.input_mass(), batches, "saw-frame timber"),
                )
            } else {
                let (craft, batches) = manual_craft_plan_for_output(
                    registries,
                    input.commodity(),
                    input.mass(),
                    "woodworking saw component planning",
                );
                let timber = if craft.input().material() == MATERIAL_WOOD {
                    checked_mass_times(craft.input_mass(), batches, "saw-component timber")
                } else {
                    Mass::ZERO
                };
                (
                    execute_manual_craft_batches(
                        registries,
                        state,
                        craft.process(),
                        raw,
                        parts,
                        batches,
                        "woodworking saw component",
                    ),
                    timber,
                )
            };
        attention = attention
            .checked_add(duration.value())
            .unwrap_or_else(|| panic!("woodworking saw setup overflowed"));
        raw_timber = raw_timber
            .checked_add(input_timber)
            .unwrap_or_else(|| panic!("woodworking saw setup timber overflowed"));
    }
    let equipment =
        validate_assemble_equipment(registries, state, EQUIPMENT_TIMBER_FRAME_SAW_BENCH, parts)
            .unwrap_or_else(|error| panic!("woodworking saw assembly failed: {error}"))
            .commit(state)
            .unwrap_or_else(|error| panic!("woodworking saw assembly commit failed: {error}"));
    SawSetup {
        equipment,
        attention_ticks: attention,
        raw_timber,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WoodworkingInvestmentPreference {
    ConserveScarceCopper,
    ConserveTimber,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WoodworkingRouteOutcome {
    production_ticks: u64,
    maintenance_ticks: u64,
    maintenance_services: u64,
    project_timber: Mass,
    boards: Mass,
    chips: Mass,
    final_condition_ppm: u32,
    saw_batches: u64,
    adze_batches: u64,
    saw_services: u64,
    adze_services: u64,
    fallback_due_to_copper: bool,
}

impl WoodworkingRouteOutcome {
    fn active_ticks(self) -> u64 {
        self.production_ticks
            .checked_add(self.maintenance_ticks)
            .unwrap_or_else(|| panic!("woodworking route active-time overflowed"))
    }
}

fn service_adze_if_critical(
    registries: &Registries,
    state: &mut AppState,
    raw: StockpileId,
    replacement: StockpileId,
    spent: StockpileId,
    adze: EquipmentId,
) -> Option<u64> {
    let definition = registries
        .equipment()
        .get_equipment(EQUIPMENT_STONE_WOODWORKING_ADZE)
        .unwrap_or_else(|| panic!("woodworking adze definition disappeared"));
    let condition = state
        .equipment()
        .get_equipment(adze)
        .map(|record| record.condition())
        .unwrap_or_else(|| panic!("woodworking adze disappeared before service check"));
    if definition.maintenance_thresholds().classify(condition) != MaintenanceBand::Critical {
        return None;
    }
    let preparation = execute_manual_craft_batches(
        registries,
        state,
        PROCESS_KNAP_STONE_TOOL,
        raw,
        replacement,
        1,
        "woodworking adze replacement edge",
    );
    let resolution = resolve_equipment_maintenance(
        registries,
        state,
        EquipmentMaintenanceRequest::new(adze, replacement, spent),
    )
    .unwrap_or_else(|error| panic!("woodworking adze maintenance resolution failed: {error}"));
    let start = validate_equipment_maintenance(registries, state, resolution)
        .unwrap_or_else(|error| panic!("woodworking adze maintenance validation failed: {error}"));
    let start_outcome = start
        .commit(state)
        .unwrap_or_else(|error| panic!("woodworking adze maintenance commit failed: {error}"));
    assert_eq!(start_outcome.equipment(), adze);
    let (service, completed) =
        finish_active_equipment_maintenance(registries, state, "woodworking adze service");
    assert_eq!(completed.equipment(), adze);
    Some(
        preparation
            .value()
            .checked_add(service)
            .unwrap_or_else(|| panic!("woodworking maintenance attention overflowed")),
    )
}

fn execute_adze_pipeline(
    registries: &Registries,
    state: &mut AppState,
    raw: StockpileId,
    output: StockpileId,
    replacement: StockpileId,
    spent: StockpileId,
    adze: EquipmentId,
    batches: u64,
) -> WoodworkingRouteOutcome {
    let mut production_ticks = 0_u64;
    let mut maintenance_ticks = 0_u64;
    let mut maintenance_services = 0_u64;
    for _ in 0..batches {
        if let Some(ticks) =
            service_adze_if_critical(registries, state, raw, replacement, spent, adze)
        {
            maintenance_ticks = maintenance_ticks
                .checked_add(ticks)
                .unwrap_or_else(|| panic!("woodworking adze maintenance total overflowed"));
            maintenance_services += 1;
        }
        let duration = execute_manual_craft(
            registries,
            state,
            select_manual_craft_request(
                registries,
                state,
                PROCESS_SHAPE_WOOD_BOARDS,
                raw,
                1,
                "woodworking adze pipeline",
            )
            .with_equipment(adze),
            output,
            "woodworking adze pipeline",
        );
        production_ticks = production_ticks
            .checked_add(duration.value())
            .unwrap_or_else(|| panic!("woodworking adze production duration overflowed"));
    }
    let output_record = state
        .inventory()
        .get_stockpile(output)
        .unwrap_or_else(|| panic!("woodworking adze output stockpile disappeared"));
    let condition = state
        .equipment()
        .get_equipment(adze)
        .map(|record| record.condition())
        .unwrap_or_else(|| panic!("woodworking adze disappeared after pipeline"));
    WoodworkingRouteOutcome {
        production_ticks,
        maintenance_ticks,
        maintenance_services,
        project_timber: checked_mass_times(
            Mass::from_milligrams(1_000_000),
            batches,
            "adze project",
        ),
        boards: output_record.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_BOARD)),
        chips: output_record.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_CHIP)),
        final_condition_ppm: condition.parts_per_million(),
        saw_batches: 0,
        adze_batches: batches,
        saw_services: 0,
        adze_services: maintenance_services,
        fallback_due_to_copper: false,
    }
}

fn execute_saw_pipeline(
    registries: &Registries,
    state: &mut AppState,
    raw: StockpileId,
    output: StockpileId,
    saw: EquipmentId,
    saw_replacement: StockpileId,
    saw_spent: StockpileId,
    adze_replacement: StockpileId,
    adze_spent: StockpileId,
    adze: EquipmentId,
    target_boards: Mass,
    blade_input: Mass,
) -> WoodworkingRouteOutcome {
    let mut production_ticks = 0_u64;
    let mut maintenance_ticks = 0_u64;
    let mut saw_services = 0_u64;
    let mut saw_batches = 0_u64;
    let mut fallback_due_to_copper = false;
    let saw_definition = registries
        .equipment()
        .get_equipment(EQUIPMENT_TIMBER_FRAME_SAW_BENCH)
        .unwrap_or_else(|| panic!("woodworking frame-saw definition disappeared"));
    loop {
        let boards = state
            .inventory()
            .get_stockpile(output)
            .map(|record| record.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_BOARD)))
            .unwrap_or_else(|| panic!("woodworking saw output stockpile disappeared"));
        if boards >= target_boards {
            break;
        }
        let saw_condition = state
            .equipment()
            .get_equipment(saw)
            .map(|record| record.condition())
            .unwrap_or_else(|| panic!("woodworking saw disappeared during pipeline"));
        if saw_definition
            .maintenance_thresholds()
            .classify(saw_condition)
            == MaintenanceBand::Critical
        {
            let copper_available = state
                .inventory()
                .get_stockpile(raw)
                .map(|record| {
                    record.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL))
                })
                .unwrap_or_else(|| panic!("woodworking raw stockpile disappeared"));
            if copper_available < blade_input {
                fallback_due_to_copper = true;
                break;
            }
            let preparation = execute_manual_craft_batches(
                registries,
                state,
                PROCESS_COLD_WORK_COPPER_SAW_BLADE,
                raw,
                saw_replacement,
                1,
                "woodworking saw replacement blade",
            );
            let resolution = resolve_equipment_maintenance(
                registries,
                state,
                EquipmentMaintenanceRequest::new(saw, saw_replacement, saw_spent),
            )
            .unwrap_or_else(|error| {
                panic!("woodworking saw maintenance resolution failed: {error}")
            });
            let start = validate_equipment_maintenance(registries, state, resolution)
                .unwrap_or_else(|error| {
                    panic!("woodworking saw maintenance validation failed: {error}")
                });
            let start_outcome = start.commit(state).unwrap_or_else(|error| {
                panic!("woodworking saw maintenance commit failed: {error}")
            });
            assert_eq!(start_outcome.equipment(), saw);
            let (service, completed) =
                finish_active_equipment_maintenance(registries, state, "woodworking saw service");
            assert_eq!(completed.equipment(), saw);
            maintenance_ticks = maintenance_ticks
                .checked_add(preparation.value())
                .and_then(|ticks| ticks.checked_add(service))
                .unwrap_or_else(|| panic!("woodworking saw maintenance duration overflowed"));
            saw_services += 1;
        }
        let duration = execute_manual_craft(
            registries,
            state,
            select_manual_craft_request(
                registries,
                state,
                PROCESS_SAW_WOOD_BOARDS,
                raw,
                1,
                "woodworking saw pipeline",
            )
            .with_equipment(saw),
            output,
            "woodworking saw pipeline",
        );
        production_ticks = production_ticks
            .checked_add(duration.value())
            .unwrap_or_else(|| panic!("woodworking saw production duration overflowed"));
        saw_batches += 1;
    }

    let boards_after_saw = state
        .inventory()
        .get_stockpile(output)
        .map(|record| record.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_BOARD)))
        .unwrap_or_else(|| panic!("woodworking saw output stockpile disappeared before fallback"));
    let remaining_boards = target_boards
        .checked_sub(boards_after_saw)
        .unwrap_or(Mass::ZERO);
    let adze_board_mass = registries
        .crafting()
        .get_manual(PROCESS_SHAPE_WOOD_BOARDS)
        .map(|definition| {
            authored_output_mass(definition, CommodityKey::new(MATERIAL_WOOD, FORM_BOARD))
        })
        .unwrap_or_else(|| panic!("woodworking adze board process disappeared during fallback"));
    let adze_batches = if remaining_boards.is_zero() {
        0
    } else {
        remaining_boards
            .milligrams()
            .div_ceil(adze_board_mass.milligrams())
    };
    let adze_tail = (adze_batches > 0).then(|| {
        execute_adze_pipeline(
            registries,
            state,
            raw,
            output,
            adze_replacement,
            adze_spent,
            adze,
            adze_batches,
        )
    });
    if let Some(tail) = adze_tail {
        production_ticks = production_ticks
            .checked_add(tail.production_ticks)
            .unwrap_or_else(|| panic!("woodworking hybrid production duration overflowed"));
        maintenance_ticks = maintenance_ticks
            .checked_add(tail.maintenance_ticks)
            .unwrap_or_else(|| panic!("woodworking hybrid maintenance duration overflowed"));
    }
    let output_record = state
        .inventory()
        .get_stockpile(output)
        .unwrap_or_else(|| panic!("woodworking saw output stockpile disappeared"));
    let condition = state
        .equipment()
        .get_equipment(saw)
        .map(|record| record.condition())
        .unwrap_or_else(|| panic!("woodworking saw disappeared after pipeline"));
    WoodworkingRouteOutcome {
        production_ticks,
        maintenance_ticks,
        maintenance_services: saw_services
            .checked_add(adze_tail.map_or(0, |tail| tail.maintenance_services))
            .unwrap_or_else(|| panic!("woodworking hybrid service count overflowed")),
        project_timber: checked_mass_times(
            Mass::from_milligrams(1_000_000),
            saw_batches
                .checked_add(adze_batches)
                .unwrap_or_else(|| panic!("woodworking hybrid batch count overflowed")),
            "saw-assisted project",
        ),
        boards: output_record.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_BOARD)),
        chips: output_record.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_CHIP)),
        final_condition_ppm: condition.parts_per_million(),
        saw_batches,
        adze_batches,
        saw_services,
        adze_services: adze_tail.map_or(0, |tail| tail.maintenance_services),
        fallback_due_to_copper,
    }
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
    let immediate_demand_scale = 3 + mix64(seed ^ 0x574F_4F44_5052_4F4A) % 10;
    let queued_demand_scale = mix64(seed ^ 0x574F_4F44_5155_4555) % 51;
    let pipeline_demand_scale = immediate_demand_scale
        .checked_add(queued_demand_scale)
        .unwrap_or_else(|| panic!("woodworking demand horizon overflowed"));
    let immediate_board_demand = Mass::from_milligrams(
        adze_board_mass_per_batch
            .milligrams()
            .checked_mul(immediate_demand_scale)
            .unwrap_or_else(|| panic!("woodworking board demand overflowed")),
    );
    let pipeline_board_demand = Mass::from_milligrams(
        adze_board_mass_per_batch
            .milligrams()
            .checked_mul(pipeline_demand_scale)
            .unwrap_or_else(|| panic!("woodworking pipeline board demand overflowed")),
    );
    let adze_batches = pipeline_board_demand
        .milligrams()
        .div_ceil(adze_board_mass_per_batch.milligrams());
    let saw_batches = pipeline_board_demand
        .milligrams()
        .div_ceil(saw_board_mass_per_batch.milligrams());
    assert_eq!(adze_batches, pipeline_demand_scale);
    let blade_input = registries
        .crafting()
        .get_manual(PROCESS_COLD_WORK_COPPER_SAW_BLADE)
        .map(|definition| definition.input_mass())
        .unwrap_or_else(|| panic!("woodworking saw-blade process disappeared"));
    let copper_available = match mix64(seed ^ 0x574F_4F44_434F_5050) % 4 {
        0 => Mass::from_milligrams(20_000),
        1 => blade_input,
        2 => Mass::from_milligrams(100_000),
        _ => Mass::from_milligrams(200_000),
    };
    let saw_fundable = copper_available >= blade_input;
    let protected_copper_reserve = Mass::from_milligrams(40_000);

    let mut state = AppState::new(WorldSeed::new(seed ^ 0x574F_4F44_574F_524C));
    let raw = add_solid_stockpile(&mut state, Mass::from_milligrams(80_500_000));
    seed_lot(
        registries,
        &mut state,
        raw,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(5_000_000),
        ROOM_TEMPERATURE,
    );
    seed_lot(
        registries,
        &mut state,
        raw,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(75_000_000),
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
    let output = add_solid_stockpile(&mut state, Mass::from_milligrams(65_000_000));
    let adze_replacement = add_solid_stockpile(&mut state, Mass::from_milligrams(2_000_000));
    let adze_spent = add_solid_stockpile(&mut state, Mass::from_milligrams(5_000_000));
    let saw_replacement = add_solid_stockpile(&mut state, Mass::from_milligrams(500_000));
    let saw_spent = add_solid_stockpile(&mut state, Mass::from_milligrams(500_000));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("woodworking initial matter audit failed: {error}"))
        .total();
    initialize_player_survival(registries, &mut state)
        .unwrap_or_else(|error| panic!("woodworking survival setup failed: {error}"));

    let (adze, adze_setup) = assemble_adze(registries, &mut state, raw, adze_parts);
    let immediate_adze_request = select_manual_craft_request(
        registries,
        &state,
        PROCESS_SHAPE_WOOD_BOARDS,
        raw,
        immediate_demand_scale,
        "woodworking adze projection",
    )
    .with_equipment(adze);
    let immediate_adze_projection =
        resolve_manual_craft(registries, &state, &immediate_adze_request)
            .unwrap_or_else(|error| panic!("woodworking adze projection failed: {error}"));
    let bare_projection = resolve_manual_craft(
        registries,
        &state,
        &select_manual_craft_request(
            registries,
            &state,
            PROCESS_SHAPE_WOOD_BOARDS,
            raw,
            immediate_demand_scale,
            "woodworking bare projection",
        ),
    )
    .unwrap_or_else(|error| panic!("woodworking bare projection failed: {error}"));
    assert_eq!(
        immediate_adze_projection.output_streams(),
        bare_projection.output_streams()
    );
    assert!(immediate_adze_projection.duration() < bare_projection.duration());
    assert_eq!(
        projected_board_mass(&immediate_adze_projection),
        immediate_board_demand
    );

    let common_state = state;
    let mut adze_state = common_state.clone();
    let adze_route = execute_adze_pipeline(
        registries,
        &mut adze_state,
        raw,
        output,
        adze_replacement,
        adze_spent,
        adze,
        adze_batches,
    );
    assert!(adze_route.boards >= pipeline_board_demand);
    assert_eq!(
        adze_route.boards.checked_add(adze_route.chips),
        Some(adze_route.project_timber)
    );
    validate_loaded_state(registries, &adze_state)
        .unwrap_or_else(|error| panic!("woodworking adze counterfactual state invalid: {error}"));

    let saw_counterfactual = saw_fundable.then(|| {
        let mut saw_state = common_state.clone();
        let setup = assemble_saw(registries, &mut saw_state, raw, saw_parts, adze);
        let route = execute_saw_pipeline(
            registries,
            &mut saw_state,
            raw,
            output,
            setup.equipment,
            saw_replacement,
            saw_spent,
            adze_replacement,
            adze_spent,
            adze,
            pipeline_board_demand,
            blade_input,
        );
        assert!(route.boards >= pipeline_board_demand);
        assert_eq!(
            route.boards.checked_add(route.chips),
            Some(route.project_timber)
        );
        validate_loaded_state(registries, &saw_state).unwrap_or_else(|error| {
            panic!("woodworking saw counterfactual state invalid: {error}")
        });
        (saw_state, setup, route)
    });

    let saw_total_timber = saw_counterfactual.as_ref().map(|(_, setup, route)| {
        setup
            .raw_timber
            .checked_add(route.project_timber)
            .unwrap_or_else(|| panic!("woodworking saw total timber overflowed"))
    });
    let saw_total_attention = saw_counterfactual.as_ref().map(|(_, setup, route)| {
        setup
            .attention_ticks
            .checked_add(route.active_ticks())
            .unwrap_or_else(|| panic!("woodworking saw total attention overflowed"))
    });
    let saw_setup_timber = saw_counterfactual
        .as_ref()
        .map_or(0, |(_, setup, _)| setup.raw_timber.milligrams());
    let saw_actual_batches = saw_counterfactual
        .as_ref()
        .map_or(0, |(_, _, route)| route.saw_batches);
    let saw_fallback_adze_batches = saw_counterfactual
        .as_ref()
        .map_or(0, |(_, _, route)| route.adze_batches);
    let saw_fallback_due_to_copper = saw_counterfactual
        .as_ref()
        .is_some_and(|(_, _, route)| route.fallback_due_to_copper);
    let saw_service_count = saw_counterfactual
        .as_ref()
        .map_or(0, |(_, _, route)| route.saw_services);
    let saw_fallback_adze_service_count = saw_counterfactual
        .as_ref()
        .map_or(0, |(_, _, route)| route.adze_services);
    let adze_total_attention = adze_route.active_ticks();
    let saw_attention_payback =
        saw_total_attention.is_some_and(|ticks| ticks < adze_total_attention);
    let saw_net_timber_payback =
        saw_total_timber.is_some_and(|mass| mass < adze_route.project_timber);
    let saw_copper_consumed = saw_counterfactual
        .as_ref()
        .map_or(Mass::ZERO, |(_, _, route)| {
            checked_mass_times(
                blade_input,
                route
                    .saw_services
                    .checked_add(1)
                    .unwrap_or_else(|| panic!("woodworking saw blade count overflowed")),
                "saw lifecycle copper",
            )
        });
    let copper_after_saw = copper_available
        .checked_sub(saw_copper_consumed)
        .unwrap_or(Mass::ZERO);
    let reserve_safe = saw_fundable && copper_after_saw >= protected_copper_reserve;
    let invest_in_saw = match preference {
        WoodworkingInvestmentPreference::ConserveTimber => saw_fundable && saw_net_timber_payback,
        WoodworkingInvestmentPreference::ConserveScarceCopper => {
            saw_fundable && reserve_safe && saw_attention_payback
        }
    };
    let reason = if !saw_fundable {
        "copper-supply-limited"
    } else if preference == WoodworkingInvestmentPreference::ConserveScarceCopper && !reserve_safe {
        "copper-reserve-protected"
    } else if preference == WoodworkingInvestmentPreference::ConserveTimber
        && !saw_net_timber_payback
    {
        "pipeline-too-short-for-net-timber-payback"
    } else if !saw_attention_payback {
        "pipeline-too-short-for-attention-payback"
    } else if preference == WoodworkingInvestmentPreference::ConserveTimber {
        "pipeline-net-timber-payback"
    } else {
        "surplus-copper-attention-payback"
    };
    let (choice, state, selected_route, selected_setup_ticks, selected_total_timber) =
        if invest_in_saw {
            let (saw_state, setup, route) = saw_counterfactual.unwrap_or_else(|| {
                unreachable!("saw investment requires a fundable counterfactual")
            });
            (
                "frame-saw",
                saw_state,
                route,
                adze_setup
                    .checked_add(setup.attention_ticks)
                    .unwrap_or_else(|| panic!("woodworking saw setup total overflowed")),
                setup
                    .raw_timber
                    .checked_add(route.project_timber)
                    .unwrap_or_else(|| panic!("woodworking selected saw timber overflowed")),
            )
        } else {
            (
                "stone-adze",
                adze_state,
                adze_route,
                adze_setup,
                adze_route.project_timber,
            )
        };

    let output_record = state
        .inventory()
        .get_stockpile(output)
        .unwrap_or_else(|| panic!("woodworking output stockpile disappeared"));
    let boards = output_record.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_BOARD));
    let chips = output_record.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_CHIP));
    assert_eq!(
        boards.checked_add(chips),
        Some(selected_route.project_timber),
        "woodworking selected path must conserve the project timber"
    );
    assert!(
        boards >= pipeline_board_demand,
        "woodworking selected path must satisfy the visible board-demand pipeline"
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("woodworking final matter audit failed: {error}"))
            .total(),
        matter_before
    );
    validate_loaded_state(registries, &state)
        .unwrap_or_else(|error| panic!("woodworking final state invalid: {error}"));

    let selected_attention = selected_setup_ticks
        .checked_add(selected_route.active_ticks())
        .unwrap_or_else(|| panic!("woodworking selected attention overflowed"));
    let adze_attention = adze_setup
        .checked_add(adze_total_attention)
        .unwrap_or_else(|| panic!("woodworking adze attention overflowed"));
    let attention_delta = i128::from(selected_attention) - i128::from(adze_attention);
    let timber_delta = i128::from(selected_total_timber.milligrams())
        - i128::from(adze_route.project_timber.milligrams());
    let board_surplus = boards
        .checked_sub(pipeline_board_demand)
        .unwrap_or_else(|| unreachable!("selected woodworking path satisfies board demand"));
    let saw_route_attention = saw_total_attention.unwrap_or(0);
    let saw_route_timber = saw_total_timber.map_or(0, Mass::milligrams);
    let adze_route_time = format_physical_duration(registries, adze_total_attention);
    let saw_route_time = format_physical_duration(registries, saw_route_attention);
    let selected_setup_time = format_physical_duration(registries, selected_setup_ticks);
    let selected_active_time = format_physical_duration(registries, selected_route.active_ticks());
    let selected_total_time = format_physical_duration(registries, selected_attention);
    let attention_delta_time = signed_physical_duration(registries, attention_delta);
    let saw_counterfactual_tradeoff = match (saw_total_attention, saw_total_timber) {
        (Some(saw_attention), Some(saw_timber)) => {
            let saw_attention_delta = i128::from(saw_attention) - i128::from(adze_total_attention);
            let saw_timber_delta = i128::from(saw_timber.milligrams())
                - i128::from(adze_route.project_timber.milligrams());
            format!(
                "attention:{saw_attention_delta:+}t/{} timber:{saw_timber_delta:+}mg",
                signed_physical_duration(registries, saw_attention_delta)
            )
        }
        _ => "unavailable:copper".to_owned(),
    };
    let bare_immediate_time =
        format_physical_duration(registries, bare_projection.duration().value());
    let adze_immediate_time =
        format_physical_duration(registries, immediate_adze_projection.duration().value());
    reviewln!(
        "WOODWORKING EXPERIENCE seed=0x{seed:016X} behavior=0x{behavior_seed:016X} sample={} demand=[immediate:{}mg queued:{}mg pipeline:{}mg boards] preference={} copper=[available:{}mg blade:{}mg protected-reserve:{}mg lifecycle-spend:{}mg after-saw:{}mg] routes=[adze:{}logs timber:{}mg attention:{}t/{adze_route_time} production:{}t maintenance:{}t/{}services final-condition:{}ppm; saw-assisted:min-saw-logs:{} fundable:{saw_fundable} setup-timber:{}mg actual=[saw:{} adze-fallback:{} fallback-copper:{} saw-services:{} adze-services:{}] timber:{}mg attention:{}t/{saw_route_time} attention-payback:{saw_attention_payback} net-timber-payback:{saw_net_timber_payback} counterfactual-vs-adze=[{saw_counterfactual_tradeoff}]] choice={choice} reason={reason} selected=[setup:{}t/{selected_setup_time} active:{}t/{selected_active_time} total:{}t/{selected_total_time} timber:{}mg project-timber:{}mg boards:{}mg surplus:{}mg chips:{}mg condition:{}ppm] selected-vs-adze=[attention:{:+}t/{attention_delta_time} timber:{:+}mg] immediate-baseline=[bare:{}t/{bare_immediate_time} adze:{}t/{adze_immediate_time}] matter=conserved",
        focused_probe_role_label(case.role()),
        immediate_board_demand.milligrams(),
        pipeline_board_demand
            .checked_sub(immediate_board_demand)
            .unwrap_or_else(|| unreachable!("pipeline demand includes immediate demand"))
            .milligrams(),
        pipeline_board_demand.milligrams(),
        preference.label(),
        copper_available.milligrams(),
        blade_input.milligrams(),
        protected_copper_reserve.milligrams(),
        saw_copper_consumed.milligrams(),
        copper_after_saw.milligrams(),
        adze_batches,
        adze_route.project_timber.milligrams(),
        adze_total_attention,
        adze_route.production_ticks,
        adze_route.maintenance_ticks,
        adze_route.maintenance_services,
        adze_route.final_condition_ppm,
        saw_batches,
        saw_setup_timber,
        saw_actual_batches,
        saw_fallback_adze_batches,
        saw_fallback_due_to_copper,
        saw_service_count,
        saw_fallback_adze_service_count,
        saw_route_timber,
        saw_route_attention,
        selected_setup_ticks,
        selected_route.active_ticks(),
        selected_attention,
        selected_total_timber.milligrams(),
        selected_route.project_timber.milligrams(),
        boards.milligrams(),
        board_surplus.milligrams(),
        chips.milligrams(),
        selected_route.final_condition_ppm,
        attention_delta,
        timber_delta,
        bare_projection.duration().value(),
        immediate_adze_projection.duration().value(),
    );
}
