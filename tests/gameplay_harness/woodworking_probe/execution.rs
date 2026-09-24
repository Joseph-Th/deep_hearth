//! Executes woodworking setup, production, fallback, and maintenance through canonical game paths.

use super::*;

pub(super) fn assemble_adze(
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
        let (craft, batches, source) = manual_craft_plan_for_available_output(
            registries,
            state,
            &[raw],
            input.commodity(),
            input.mass(),
            "woodworking adze component planning",
        );
        let duration = execute_manual_craft_batches(
            registries,
            state,
            craft.process(),
            source,
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
pub(super) struct SawSetup {
    pub(super) equipment: EquipmentId,
    pub(super) attention_ticks: u64,
    pub(super) raw_timber: Mass,
}

pub(super) fn checked_mass_times(mass: Mass, count: u64, context: &'static str) -> Mass {
    Mass::from_milligrams(
        mass.milligrams()
            .checked_mul(count)
            .unwrap_or_else(|| panic!("woodworking {context} mass overflowed")),
    )
}

fn saw_frame_board_batches(registries: &Registries, required_boards: Mass) -> u64 {
    let board_craft = registries
        .crafting()
        .get_manual(PROCESS_SHAPE_WOOD_BOARDS)
        .unwrap_or_else(|| panic!("woodworking saw-frame board route disappeared"));
    let boards_per_batch =
        authored_output_mass(board_craft, CommodityKey::new(MATERIAL_WOOD, FORM_BOARD));
    required_boards
        .milligrams()
        .div_ceil(boards_per_batch.milligrams())
}

pub(super) fn project_saw_setup_budget(
    registries: &Registries,
    state: &AppState,
    raw: StockpileId,
) -> (u64, Mass) {
    let assembly = registries
        .equipment()
        .get_equipment(EQUIPMENT_TIMBER_FRAME_SAW_BENCH)
        .and_then(|definition| definition.assembly_profile())
        .unwrap_or_else(|| panic!("woodworking frame saw lost its authored assembly"));
    assembly
        .inputs()
        .iter()
        .fold((0_u64, Mass::ZERO), |(ticks, timber), input| {
            let (duration, input_timber) =
                if input.commodity() == CommodityKey::new(MATERIAL_WOOD, FORM_BOARD) {
                    let board_craft = registries
                        .crafting()
                        .get_manual(PROCESS_SHAPE_WOOD_BOARDS)
                        .unwrap_or_else(|| panic!("woodworking saw-frame board route disappeared"));
                    let batches = saw_frame_board_batches(registries, input.mass());
                    let _ = select_manual_craft_request(
                        registries,
                        state,
                        board_craft.process(),
                        raw,
                        batches,
                        "woodworking saw pre-investment board availability",
                    );
                    let projection = project_manual_craft_equipment(
                        registries,
                        board_craft.process(),
                        NonZeroU64::new(batches)
                            .unwrap_or_else(|| unreachable!("saw frame requires board work")),
                        EQUIPMENT_STONE_WOODWORKING_ADZE,
                        Condition::PRISTINE,
                    )
                    .unwrap_or_else(|error| {
                        panic!("woodworking saw-frame adze projection failed: {error}")
                    });
                    (
                        projection.duration().value(),
                        checked_mass_times(
                            board_craft.input_mass(),
                            batches,
                            "projected saw-frame timber",
                        ),
                    )
                } else {
                    let (craft, batches, source) = manual_craft_plan_for_available_output(
                        registries,
                        state,
                        &[raw],
                        input.commodity(),
                        input.mass(),
                        "woodworking saw pre-investment component",
                    );
                    let resolution = resolve_manual_craft(
                        registries,
                        state,
                        &select_manual_craft_request(
                            registries,
                            state,
                            craft.process(),
                            source,
                            batches,
                            "woodworking saw pre-investment component",
                        ),
                    )
                    .unwrap_or_else(|error| {
                        panic!("woodworking saw component projection failed: {error}")
                    });
                    (
                        resolution.duration().value(),
                        if craft.input().material() == MATERIAL_WOOD {
                            checked_mass_times(
                                craft.input_mass(),
                                batches,
                                "projected saw-component timber",
                            )
                        } else {
                            Mass::ZERO
                        },
                    )
                };
            (
                ticks
                    .checked_add(duration)
                    .unwrap_or_else(|| panic!("woodworking saw setup projection overflowed")),
                timber
                    .checked_add(input_timber)
                    .unwrap_or_else(|| panic!("woodworking saw timber projection overflowed")),
            )
        })
}

pub(super) fn assemble_saw(
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
                let batches = saw_frame_board_batches(registries, input.mass());
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
                let (craft, batches, source) = manual_craft_plan_for_available_output(
                    registries,
                    state,
                    &[raw],
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
                        source,
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
pub(super) struct WoodworkingRouteOutcome {
    pub(super) production_ticks: u64,
    pub(super) maintenance_ticks: u64,
    pub(super) maintenance_services: u64,
    pub(super) project_timber: Mass,
    pub(super) boards: Mass,
    pub(super) chips: Mass,
    pub(super) final_condition_ppm: Option<u32>,
    pub(super) saw_batches: u64,
    pub(super) adze_batches: u64,
    pub(super) saw_services: u64,
    pub(super) adze_services: u64,
    pub(super) fallback_due_to_copper: bool,
}

impl WoodworkingRouteOutcome {
    pub(super) fn active_ticks(self) -> u64 {
        self.production_ticks
            .checked_add(self.maintenance_ticks)
            .unwrap_or_else(|| panic!("woodworking route active-time overflowed"))
    }
}

pub(super) fn execute_bare_pipeline(
    registries: &Registries,
    state: &mut AppState,
    raw: StockpileId,
    output: StockpileId,
    batches: u64,
) -> WoodworkingRouteOutcome {
    let board_process = registries
        .crafting()
        .get_manual(PROCESS_SHAPE_WOOD_BOARDS)
        .unwrap_or_else(|| panic!("woodworking bare board process disappeared"));
    let duration = execute_manual_craft_batches(
        registries,
        state,
        PROCESS_SHAPE_WOOD_BOARDS,
        raw,
        output,
        batches,
        "woodworking bare pipeline",
    );
    let output_record = state
        .inventory()
        .get_stockpile(output)
        .unwrap_or_else(|| panic!("woodworking bare output stockpile disappeared"));
    WoodworkingRouteOutcome {
        production_ticks: duration.value(),
        maintenance_ticks: 0,
        maintenance_services: 0,
        project_timber: checked_mass_times(board_process.input_mass(), batches, "bare project"),
        boards: output_record.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_BOARD)),
        chips: output_record.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_CHIP)),
        final_condition_ppm: None,
        saw_batches: 0,
        adze_batches: 0,
        saw_services: 0,
        adze_services: 0,
        fallback_due_to_copper: false,
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

#[derive(Clone, Copy)]
pub(super) struct AdzePipelinePlan {
    pub(super) raw: StockpileId,
    pub(super) output: StockpileId,
    pub(super) replacement: StockpileId,
    pub(super) spent: StockpileId,
    pub(super) adze: EquipmentId,
    pub(super) batches: u64,
}

pub(super) fn execute_adze_pipeline(
    registries: &Registries,
    state: &mut AppState,
    plan: AdzePipelinePlan,
) -> WoodworkingRouteOutcome {
    let AdzePipelinePlan {
        raw,
        output,
        replacement,
        spent,
        adze,
        batches,
    } = plan;
    let mut production_ticks = 0_u64;
    let mut maintenance_ticks = 0_u64;
    let mut maintenance_services = 0_u64;
    let board_process = registries
        .crafting()
        .get_manual(PROCESS_SHAPE_WOOD_BOARDS)
        .unwrap_or_else(|| panic!("woodworking adze board process disappeared"));
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
        project_timber: checked_mass_times(board_process.input_mass(), batches, "adze project"),
        boards: output_record.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_BOARD)),
        chips: output_record.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_CHIP)),
        final_condition_ppm: Some(condition.parts_per_million()),
        saw_batches: 0,
        adze_batches: batches,
        saw_services: 0,
        adze_services: maintenance_services,
        fallback_due_to_copper: false,
    }
}

#[derive(Clone, Copy)]
pub(super) struct SawPipelinePlan {
    pub(super) raw: StockpileId,
    pub(super) output: StockpileId,
    pub(super) saw: EquipmentId,
    pub(super) saw_replacement: StockpileId,
    pub(super) saw_spent: StockpileId,
    pub(super) adze_replacement: StockpileId,
    pub(super) adze_spent: StockpileId,
    pub(super) adze: EquipmentId,
    pub(super) target_boards: Mass,
    pub(super) blade_input: Mass,
    pub(super) protected_reserve: Mass,
}

pub(super) fn execute_saw_pipeline(
    registries: &Registries,
    state: &mut AppState,
    plan: SawPipelinePlan,
) -> WoodworkingRouteOutcome {
    let SawPipelinePlan {
        raw,
        output,
        saw,
        saw_replacement,
        saw_spent,
        adze_replacement,
        adze_spent,
        adze,
        target_boards,
        blade_input,
        protected_reserve,
    } = plan;
    let mut production_ticks = 0_u64;
    let mut maintenance_ticks = 0_u64;
    let mut saw_services = 0_u64;
    let mut saw_batches = 0_u64;
    let mut fallback_due_to_copper = false;
    let saw_board_process = registries
        .crafting()
        .get_manual(PROCESS_SAW_WOOD_BOARDS)
        .unwrap_or_else(|| panic!("woodworking saw board process disappeared"));
    let adze_board_process = registries
        .crafting()
        .get_manual(PROCESS_SHAPE_WOOD_BOARDS)
        .unwrap_or_else(|| panic!("woodworking fallback board process disappeared"));
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
            if copper_available
                .checked_sub(blade_input)
                .is_none_or(|remaining| remaining < protected_reserve)
            {
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
            AdzePipelinePlan {
                raw,
                output,
                replacement: adze_replacement,
                spent: adze_spent,
                adze,
                batches: adze_batches,
            },
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
    let saw_project_timber = checked_mass_times(
        saw_board_process.input_mass(),
        saw_batches,
        "saw-assisted project",
    );
    let adze_project_timber = checked_mass_times(
        adze_board_process.input_mass(),
        adze_batches,
        "adze fallback project",
    );
    WoodworkingRouteOutcome {
        production_ticks,
        maintenance_ticks,
        maintenance_services: saw_services
            .checked_add(adze_tail.map_or(0, |tail| tail.maintenance_services))
            .unwrap_or_else(|| panic!("woodworking hybrid service count overflowed")),
        project_timber: saw_project_timber
            .checked_add(adze_project_timber)
            .unwrap_or_else(|| panic!("woodworking hybrid project timber overflowed")),
        boards: output_record.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_BOARD)),
        chips: output_record.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_CHIP)),
        final_condition_ppm: Some(condition.parts_per_million()),
        saw_batches,
        adze_batches,
        saw_services,
        adze_services: adze_tail.map_or(0, |tail| tail.maintenance_services),
        fallback_due_to_copper,
    }
}

pub(super) fn authored_output_mass(
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

pub(super) fn projected_board_mass(resolution: &ProcessResolution) -> Mass {
    resolution
        .single_output_stream()
        .unwrap_or_else(|| panic!("woodworking projection lost its single physical output stream"))
        .outputs()
        .iter()
        .find(|output| output.commodity() == CommodityKey::new(MATERIAL_WOOD, FORM_BOARD))
        .map(|output| output.mass())
        .unwrap_or_else(|| panic!("woodworking projection lost its board output"))
}
