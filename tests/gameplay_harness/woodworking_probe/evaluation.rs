//! Builds woodworking actor decisions, matched lifecycle evidence, and replayable report outcomes.

use super::execution::{
    AdzePipelinePlan, SawPipelinePlan, SawSetup, WoodworkingRouteOutcome, assemble_adze,
    assemble_saw, authored_output_mass, checked_mass_times, execute_adze_pipeline,
    execute_bare_pipeline, execute_saw_pipeline, project_saw_setup_budget, projected_board_mass,
};
use super::*;

fn signed_physical_duration(registries: &Registries, ticks: i128) -> String {
    let magnitude = u64::try_from(ticks.unsigned_abs())
        .unwrap_or_else(|_| panic!("woodworking signed duration exceeds u64"));
    format!(
        "{}{}",
        if ticks < 0 { "-" } else { "+" },
        format_physical_duration(registries, magnitude)
    )
}

pub(super) fn run_woodworking_probe(registries: &Registries, case: FocusedProbeCase) {
    evaluate_woodworking_probe(registries, case);
}

#[derive(Clone, Copy)]
struct WoodworkingDemandPlan {
    horizon: &'static str,
    immediate_scale: u64,
    immediate_boards: Mass,
    pipeline_boards: Mass,
    adze_batches: u64,
    saw_batches: u64,
    adze_input_mass: Mass,
    saw_input_mass: Mass,
}

fn plan_woodworking_demand(registries: &Registries, seed: u64) -> WoodworkingDemandPlan {
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
    let immediate_roll = mix64(seed ^ 0x574F_4F44_5052_4F4A);
    let queued_roll = mix64(seed ^ 0x574F_4F44_5155_4555);
    let project_queue = queued_roll % 51;
    // Exercise three player-visible planning horizons instead of letting the wide queue range make
    // the equipment-free route effectively disappear from organic play. Project-scale worlds keep
    // the original distribution; the lower tail now represents genuinely small disclosed jobs.
    let (horizon, immediate_scale, queued_scale) = match project_queue {
        0..=5 => ("immediate-only", 1 + immediate_roll % 3, 0),
        6..=15 => (
            "short-queue",
            2 + immediate_roll % 4,
            1 + (queued_roll >> 8) % 6,
        ),
        _ => ("project", 3 + immediate_roll % 10, project_queue),
    };
    let pipeline_scale = immediate_scale
        .checked_add(queued_scale)
        .unwrap_or_else(|| panic!("woodworking demand horizon overflowed"));
    let immediate_boards = Mass::from_milligrams(
        adze_board_mass_per_batch
            .milligrams()
            .checked_mul(immediate_scale)
            .unwrap_or_else(|| panic!("woodworking board demand overflowed")),
    );
    let pipeline_boards = Mass::from_milligrams(
        adze_board_mass_per_batch
            .milligrams()
            .checked_mul(pipeline_scale)
            .unwrap_or_else(|| panic!("woodworking pipeline board demand overflowed")),
    );
    let adze_batches = pipeline_boards
        .milligrams()
        .div_ceil(adze_board_mass_per_batch.milligrams());
    let saw_batches = pipeline_boards
        .milligrams()
        .div_ceil(saw_board_mass_per_batch.milligrams());
    assert_eq!(adze_batches, pipeline_scale);
    WoodworkingDemandPlan {
        horizon,
        immediate_scale,
        immediate_boards,
        pipeline_boards,
        adze_batches,
        saw_batches,
        adze_input_mass: adze_board_definition.input_mass(),
        saw_input_mass: saw_board_definition.input_mass(),
    }
}

struct WoodworkingWorld {
    state: AppState,
    raw: StockpileId,
    adze_parts: StockpileId,
    saw_parts: StockpileId,
    output: StockpileId,
    adze_replacement: StockpileId,
    adze_spent: StockpileId,
    saw_replacement: StockpileId,
    saw_spent: StockpileId,
    matter_before: deep_hearth::core::quantity::AggregateMass,
    blade_input: Mass,
    copper_available: Mass,
    saw_fundable: bool,
    protected_copper_reserve: Mass,
}

fn build_woodworking_world(registries: &Registries, seed: u64) -> WoodworkingWorld {
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
    let mut state = AppState::new();
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
    WoodworkingWorld {
        state,
        raw,
        adze_parts,
        saw_parts,
        output,
        adze_replacement,
        adze_spent,
        saw_replacement,
        saw_spent,
        matter_before,
        blade_input,
        copper_available,
        saw_fundable: copper_available >= blade_input,
        // Reserve covers two future copper reinforcements; it stands in for opportunity cost.
        protected_copper_reserve: Mass::from_milligrams(40_000),
    }
}

#[derive(Clone, Copy)]
struct WoodworkingDecisionPlan {
    preference: WoodworkingInvestmentPreference,
    bare_attention: u64,
    bare_projected_boards: Mass,
    adze_budget: u64,
    saw_budget: Option<(u64, Mass)>,
    reserve_safe_now: bool,
    setup_attention_budget_met: bool,
    nominal_timber_balance: WoodworkingTimberBalance,
    invest_in_saw: bool,
    use_bare_hands: bool,
    reason: WoodworkingInvestmentReason,
}

fn project_woodworking_construction_budget(
    registries: &Registries,
    state: &AppState,
    raw: StockpileId,
    equipment: deep_hearth::equipment::EquipmentDefinitionId,
) -> (u64, Mass) {
    let profile = registries
        .equipment()
        .get_equipment(equipment)
        .and_then(|definition| definition.assembly_profile())
        .unwrap_or_else(|| panic!("woodworking equipment has an assembly route"));
    profile
        .inputs()
        .iter()
        .fold((0_u64, Mass::ZERO), |(ticks, timber), input| {
            let (craft, batches, source) = manual_craft_plan_for_available_output(
                registries,
                state,
                &[raw],
                input.commodity(),
                input.mass(),
                "woodworking pre-investment budget",
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
                    "woodworking pre-investment budget",
                ),
            )
            .unwrap_or_else(|error| panic!("available construction input resolves: {error}"));
            let input_timber = if craft.input().material() == MATERIAL_WOOD {
                checked_mass_times(craft.input_mass(), batches, "construction budget")
            } else {
                Mass::ZERO
            };
            (
                ticks
                    .checked_add(resolution.duration().value())
                    .unwrap_or_else(|| panic!("construction time fits")),
                timber
                    .checked_add(input_timber)
                    .unwrap_or_else(|| panic!("construction timber fits")),
            )
        })
}

fn plan_woodworking_investment(
    registries: &Registries,
    behavior_seed: u64,
    demand: WoodworkingDemandPlan,
    world: &WoodworkingWorld,
) -> WoodworkingDecisionPlan {
    let preference = WoodworkingInvestmentPreference::from_behavior_seed(behavior_seed);
    let bare_pipeline_projection = resolve_manual_craft(
        registries,
        &world.state,
        &select_manual_craft_request(
            registries,
            &world.state,
            PROCESS_SHAPE_WOOD_BOARDS,
            world.raw,
            demand.adze_batches,
            "woodworking bare pipeline planning",
        ),
    )
    .unwrap_or_else(|error| panic!("woodworking bare pipeline planning failed: {error}"));
    let bare_attention = bare_pipeline_projection.duration().value();
    let (adze_budget, _) = project_woodworking_construction_budget(
        registries,
        &world.state,
        world.raw,
        EQUIPMENT_STONE_WOODWORKING_ADZE,
    );
    let saw_budget = world
        .saw_fundable
        .then(|| project_saw_setup_budget(registries, &world.state, world.raw));
    let nominal_saw_timber = saw_budget.map(|(_, timber)| {
        timber
            .checked_add(checked_mass_times(
                demand.saw_input_mass,
                demand.saw_batches,
                "nominal saw work",
            ))
            .unwrap_or_else(|| panic!("nominal saw timber fits"))
    });
    let nominal_adze_timber = checked_mass_times(
        demand.adze_input_mass,
        demand.adze_batches,
        "nominal adze work",
    );
    let reserve_safe_now = world
        .copper_available
        .checked_sub(world.blade_input)
        .is_some_and(|remaining| remaining >= world.protected_copper_reserve);
    let setup_attention_budget_met = saw_budget.is_some_and(|(ticks, _)| {
        bare_attention
            >= ticks
                .checked_add(adze_budget)
                .and_then(|ticks| ticks.checked_mul(2))
                .unwrap_or_else(|| panic!("investment willingness budget fits"))
    });
    let nominal_timber_balance =
        woodworking_timber_balance(nominal_saw_timber, nominal_adze_timber);
    let (invest_in_saw, saw_reason) = woodworking_investment_decision(
        preference,
        reserve_safe_now,
        setup_attention_budget_met,
        nominal_timber_balance,
    );
    let use_bare_hands = !invest_in_saw
        && bare_attention
            < adze_budget
                .checked_mul(2)
                .unwrap_or_else(|| panic!("adze willingness budget fits"));
    WoodworkingDecisionPlan {
        preference,
        bare_attention,
        bare_projected_boards: projected_board_mass(&bare_pipeline_projection),
        adze_budget,
        saw_budget,
        reserve_safe_now,
        setup_attention_budget_met,
        nominal_timber_balance,
        invest_in_saw,
        use_bare_hands,
        reason: if use_bare_hands {
            WoodworkingInvestmentReason::BareHandsAvoidsInvestmentCost
        } else {
            saw_reason
        },
    }
}

struct SawCounterfactual {
    state: AppState,
    setup: SawSetup,
    route: WoodworkingRouteOutcome,
}

struct WoodworkingLifecycleEvidence {
    bare_state: AppState,
    bare_route: WoodworkingRouteOutcome,
    adze_state: AppState,
    adze_route: WoodworkingRouteOutcome,
    adze: EquipmentId,
    adze_setup: u64,
    bare_immediate_ticks: u64,
    adze_immediate_ticks: u64,
    saw: Option<SawCounterfactual>,
}

fn execute_bare_counterfactual(
    registries: &Registries,
    world: &WoodworkingWorld,
    demand: WoodworkingDemandPlan,
    decision: WoodworkingDecisionPlan,
) -> (AppState, WoodworkingRouteOutcome) {
    let mut state = world.state.clone();
    let route = execute_bare_pipeline(
        registries,
        &mut state,
        world.raw,
        world.output,
        demand.adze_batches,
    );
    assert_eq!(route.active_ticks(), decision.bare_attention);
    assert_eq!(route.boards, decision.bare_projected_boards);
    assert!(route.boards >= demand.pipeline_boards);
    assert_eq!(
        route.boards.checked_add(route.chips),
        Some(route.project_timber)
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("woodworking bare matter audit failed: {error}"))
            .total(),
        world.matter_before
    );
    validate_loaded_state(registries, &state)
        .unwrap_or_else(|error| panic!("woodworking bare counterfactual state invalid: {error}"));
    (state, route)
}

fn execute_saw_counterfactual(
    registries: &Registries,
    world: &WoodworkingWorld,
    demand: WoodworkingDemandPlan,
    decision: WoodworkingDecisionPlan,
    common_state: &AppState,
    adze: EquipmentId,
) -> Option<SawCounterfactual> {
    world.saw_fundable.then(|| {
        let mut state = common_state.clone();
        let setup = assemble_saw(registries, &mut state, world.raw, world.saw_parts, adze);
        let (projected_setup_ticks, projected_setup_timber) = decision
            .saw_budget
            .unwrap_or_else(|| unreachable!("fundable saw must have a setup projection"));
        assert_eq!(
            setup.attention_ticks, projected_setup_ticks,
            "pre-action saw setup projection must match canonical executed attention"
        );
        assert_eq!(
            setup.raw_timber, projected_setup_timber,
            "pre-action saw setup projection must match canonical executed timber cost"
        );
        let route = execute_saw_pipeline(
            registries,
            &mut state,
            SawPipelinePlan {
                raw: world.raw,
                output: world.output,
                saw: setup.equipment,
                saw_replacement: world.saw_replacement,
                saw_spent: world.saw_spent,
                adze_replacement: world.adze_replacement,
                adze_spent: world.adze_spent,
                adze,
                target_boards: demand.pipeline_boards,
                blade_input: world.blade_input,
                protected_reserve: if decision.preference
                    == WoodworkingInvestmentPreference::ConserveScarceCopper
                    && decision.reserve_safe_now
                {
                    world.protected_copper_reserve
                } else {
                    Mass::ZERO
                },
            },
        );
        assert!(route.boards >= demand.pipeline_boards);
        assert_eq!(
            route.boards.checked_add(route.chips),
            Some(route.project_timber)
        );
        validate_loaded_state(registries, &state).unwrap_or_else(|error| {
            panic!("woodworking saw counterfactual state invalid: {error}")
        });
        SawCounterfactual {
            state,
            setup,
            route,
        }
    })
}

fn execute_woodworking_lifecycle(
    registries: &Registries,
    world: &WoodworkingWorld,
    demand: WoodworkingDemandPlan,
    decision: WoodworkingDecisionPlan,
) -> WoodworkingLifecycleEvidence {
    let (bare_state, bare_route) = execute_bare_counterfactual(registries, world, demand, decision);

    let mut common_state = world.state.clone();
    let (adze, adze_setup) =
        assemble_adze(registries, &mut common_state, world.raw, world.adze_parts);
    let immediate_adze_request = select_manual_craft_request(
        registries,
        &common_state,
        PROCESS_SHAPE_WOOD_BOARDS,
        world.raw,
        demand.immediate_scale,
        "woodworking adze projection",
    )
    .with_equipment(adze);
    let immediate_adze_projection =
        resolve_manual_craft(registries, &common_state, &immediate_adze_request)
            .unwrap_or_else(|error| panic!("woodworking adze projection failed: {error}"));
    let bare_projection = resolve_manual_craft(
        registries,
        &common_state,
        &select_manual_craft_request(
            registries,
            &common_state,
            PROCESS_SHAPE_WOOD_BOARDS,
            world.raw,
            demand.immediate_scale,
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
        demand.immediate_boards
    );

    let mut adze_state = common_state.clone();
    let adze_route = execute_adze_pipeline(
        registries,
        &mut adze_state,
        AdzePipelinePlan {
            raw: world.raw,
            output: world.output,
            replacement: world.adze_replacement,
            spent: world.adze_spent,
            adze,
            batches: demand.adze_batches,
        },
    );
    assert!(adze_route.boards >= demand.pipeline_boards);
    assert_eq!(
        adze_route.boards.checked_add(adze_route.chips),
        Some(adze_route.project_timber)
    );
    validate_loaded_state(registries, &adze_state)
        .unwrap_or_else(|error| panic!("woodworking adze counterfactual state invalid: {error}"));

    let saw = execute_saw_counterfactual(registries, world, demand, decision, &common_state, adze);

    WoodworkingLifecycleEvidence {
        bare_state,
        bare_route,
        adze_state,
        adze_route,
        adze,
        adze_setup,
        bare_immediate_ticks: bare_projection.duration().value(),
        adze_immediate_ticks: immediate_adze_projection.duration().value(),
        saw,
    }
}

#[derive(Clone, Copy)]
struct WoodworkingLifecycleMetrics {
    adze_total_attention: u64,
    saw_total_timber: Option<Mass>,
    saw_total_attention: Option<u64>,
    saw_setup_timber: u64,
    saw_actual_batches: u64,
    saw_fallback_adze_batches: u64,
    saw_fallback_due_to_copper: bool,
    saw_service_count: u64,
    saw_fallback_adze_service_count: u64,
    saw_attention_payback: bool,
    actual_timber_balance: WoodworkingTimberBalance,
    saw_timber_saving: bool,
    saw_timber_neutral: bool,
    saw_copper_consumed: Mass,
    copper_after_saw: Mass,
}

fn assert_woodworking_maintained_witness(
    case: FocusedProbeCase,
    decision: WoodworkingDecisionPlan,
    evidence: &WoodworkingLifecycleEvidence,
    metrics: WoodworkingLifecycleMetrics,
) {
    if case.role() == FocusedProbeRole::MaintainedCoverage && case.seed() == 12 {
        assert!(evidence.adze_route.maintenance_services > 0);
        assert!(
            decision
                .bare_attention
                .saturating_sub(metrics.adze_total_attention)
                > evidence.adze_setup + evidence.adze_route.maintenance_ticks,
            "long-order adze benefit must survive wear and service, not approach bare-hand cost"
        );
    }
    match (case.role(), case.seed()) {
        (FocusedProbeRole::MaintainedAnchor, 1) => {
            assert_eq!(
                decision.reason,
                WoodworkingInvestmentReason::PipelineNetTimberSaving
            );
            assert!(metrics.saw_fallback_due_to_copper);
        }
        (FocusedProbeRole::MaintainedCoverage, 3 | 12) => {
            assert_eq!(
                decision.reason,
                WoodworkingInvestmentReason::CopperSupplyLimited
            );
        }
        (FocusedProbeRole::MaintainedCoverage, 4) => assert_eq!(
            decision.reason,
            WoodworkingInvestmentReason::PipelineTimberNeutralWithinSetupBudget
        ),
        (FocusedProbeRole::MaintainedCoverage, 6) => {
            assert_eq!(
                decision.reason,
                WoodworkingInvestmentReason::CopperReserveProtected
            );
            assert!(decision.bare_attention >= metrics.adze_total_attention);
        }
        (FocusedProbeRole::MaintainedCoverage, 250) => {
            assert_eq!(
                decision.reason,
                WoodworkingInvestmentReason::BareHandsAvoidsInvestmentCost
            );
            assert!(decision.bare_attention < metrics.adze_total_attention);
        }
        (FocusedProbeRole::MaintainedCoverage, 0x36F7_E3A2_7870_3A8A) => {
            assert_eq!(
                decision.reason,
                WoodworkingInvestmentReason::PipelineNetTimberSaving
            );
            assert!(metrics.saw_service_count > 0);
        }
        _ => {}
    }
}

fn evaluate_woodworking_lifecycle(
    case: FocusedProbeCase,
    world: &WoodworkingWorld,
    decision: WoodworkingDecisionPlan,
    evidence: &WoodworkingLifecycleEvidence,
) -> WoodworkingLifecycleMetrics {
    let saw_total_timber = evidence.saw.as_ref().map(|saw| {
        saw.setup
            .raw_timber
            .checked_add(saw.route.project_timber)
            .unwrap_or_else(|| panic!("woodworking saw total timber overflowed"))
    });
    let saw_total_attention = evidence.saw.as_ref().map(|saw| {
        saw.setup
            .attention_ticks
            .checked_add(evidence.adze_setup)
            .and_then(|ticks| ticks.checked_add(saw.route.active_ticks()))
            .unwrap_or_else(|| panic!("woodworking saw total attention overflowed"))
    });
    let adze_total_attention = evidence
        .adze_setup
        .checked_add(evidence.adze_route.active_ticks())
        .unwrap_or_else(|| panic!("woodworking adze lifecycle attention overflowed"));
    let actual_timber_balance =
        woodworking_timber_balance(saw_total_timber, evidence.adze_route.project_timber);
    let saw_service_count = evidence
        .saw
        .as_ref()
        .map_or(0, |saw| saw.route.saw_services);
    let saw_fallback_due_to_copper = evidence
        .saw
        .as_ref()
        .is_some_and(|saw| saw.route.fallback_due_to_copper);
    let metrics = WoodworkingLifecycleMetrics {
        adze_total_attention,
        saw_total_timber,
        saw_total_attention,
        saw_setup_timber: evidence
            .saw
            .as_ref()
            .map_or(0, |saw| saw.setup.raw_timber.milligrams()),
        saw_actual_batches: evidence.saw.as_ref().map_or(0, |saw| saw.route.saw_batches),
        saw_fallback_adze_batches: evidence
            .saw
            .as_ref()
            .map_or(0, |saw| saw.route.adze_batches),
        saw_fallback_due_to_copper,
        saw_service_count,
        saw_fallback_adze_service_count: evidence
            .saw
            .as_ref()
            .map_or(0, |saw| saw.route.adze_services),
        saw_attention_payback: saw_total_attention
            .is_some_and(|ticks| ticks < adze_total_attention),
        actual_timber_balance,
        saw_timber_saving: actual_timber_balance == WoodworkingTimberBalance::Saving,
        saw_timber_neutral: actual_timber_balance == WoodworkingTimberBalance::Neutral,
        saw_copper_consumed: evidence.saw.as_ref().map_or(Mass::ZERO, |saw| {
            checked_mass_times(
                world.blade_input,
                saw.route
                    .saw_services
                    .checked_add(1)
                    .unwrap_or_else(|| panic!("woodworking saw blade count overflowed")),
                "saw lifecycle copper",
            )
        }),
        copper_after_saw: Mass::ZERO,
    };
    let metrics = WoodworkingLifecycleMetrics {
        copper_after_saw: world
            .copper_available
            .checked_sub(metrics.saw_copper_consumed)
            .unwrap_or(Mass::ZERO),
        ..metrics
    };
    assert_woodworking_maintained_witness(case, decision, evidence, metrics);
    metrics
}

struct SelectedWoodworkingRoute {
    choice: &'static str,
    route: WoodworkingRouteOutcome,
    setup_ticks: u64,
    total_timber: Mass,
    boards: Mass,
    chips: Mass,
    board_surplus: Mass,
    attention_ticks: u64,
    attention_delta: i128,
    timber_delta: i128,
    adze_route: WoodworkingRouteOutcome,
    adze_setup: u64,
    bare_immediate_ticks: u64,
    adze_immediate_ticks: u64,
}

struct WoodworkingSelectionContext<'a> {
    registries: &'a Registries,
    world: &'a WoodworkingWorld,
    demand: WoodworkingDemandPlan,
    decision: WoodworkingDecisionPlan,
    metrics: WoodworkingLifecycleMetrics,
}

struct ChosenWoodworkingRoute {
    choice: &'static str,
    state: AppState,
    route: WoodworkingRouteOutcome,
    setup_ticks: u64,
    total_timber: Mass,
    adze_route: WoodworkingRouteOutcome,
    adze: EquipmentId,
    adze_setup: u64,
    bare_immediate_ticks: u64,
    adze_immediate_ticks: u64,
}

fn choose_woodworking_route(
    decision: WoodworkingDecisionPlan,
    lifecycle: WoodworkingLifecycleEvidence,
) -> ChosenWoodworkingRoute {
    let WoodworkingLifecycleEvidence {
        bare_state,
        bare_route,
        adze_state,
        adze_route,
        adze,
        adze_setup,
        bare_immediate_ticks,
        adze_immediate_ticks,
        saw,
    } = lifecycle;
    let (choice, state, route, setup_ticks, total_timber) = if decision.invest_in_saw {
        let saw = saw.unwrap_or_else(|| unreachable!("saw investment requires a fundable route"));
        (
            "frame-saw",
            saw.state,
            saw.route,
            adze_setup
                .checked_add(saw.setup.attention_ticks)
                .unwrap_or_else(|| panic!("woodworking saw setup total overflowed")),
            saw.setup
                .raw_timber
                .checked_add(saw.route.project_timber)
                .unwrap_or_else(|| panic!("woodworking selected saw timber overflowed")),
        )
    } else if decision.use_bare_hands {
        (
            "bare-hands",
            bare_state,
            bare_route,
            0,
            bare_route.project_timber,
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
    ChosenWoodworkingRoute {
        choice,
        state,
        route,
        setup_ticks,
        total_timber,
        adze_route,
        adze,
        adze_setup,
        bare_immediate_ticks,
        adze_immediate_ticks,
    }
}

fn select_and_validate_woodworking_route(
    context: WoodworkingSelectionContext<'_>,
    lifecycle: WoodworkingLifecycleEvidence,
) -> SelectedWoodworkingRoute {
    let WoodworkingSelectionContext {
        registries,
        world,
        demand,
        decision,
        metrics,
    } = context;
    let ChosenWoodworkingRoute {
        choice,
        state,
        route,
        setup_ticks,
        total_timber,
        adze_route,
        adze,
        adze_setup,
        bare_immediate_ticks,
        adze_immediate_ticks,
    } = choose_woodworking_route(decision, lifecycle);

    let output_record = state
        .inventory()
        .get_stockpile(world.output)
        .unwrap_or_else(|| panic!("woodworking output stockpile disappeared"));
    let boards = output_record.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_BOARD));
    let chips = output_record.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_CHIP));
    assert_eq!(
        boards.checked_add(chips),
        Some(route.project_timber),
        "woodworking selected path must conserve the project timber"
    );
    assert!(
        boards >= demand.pipeline_boards,
        "woodworking selected path must satisfy the visible board-demand pipeline"
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("woodworking final matter audit failed: {error}"))
            .total(),
        world.matter_before
    );
    validate_loaded_state(registries, &state)
        .unwrap_or_else(|error| panic!("woodworking final state invalid: {error}"));
    let attention_ticks = setup_ticks
        .checked_add(route.active_ticks())
        .unwrap_or_else(|| panic!("woodworking selected attention overflowed"));
    assert_eq!(
        state.tick().value(),
        attention_ticks,
        "selected woodworking lifecycle must account for every elapsed tick"
    );
    if decision.use_bare_hands {
        assert_eq!(attention_ticks, decision.bare_attention);
        assert_eq!(route.final_condition_ppm, None);
        assert!(state.equipment().get_equipment(adze).is_none());
    }
    SelectedWoodworkingRoute {
        choice,
        route,
        setup_ticks,
        total_timber,
        boards,
        chips,
        board_surplus: boards
            .checked_sub(demand.pipeline_boards)
            .unwrap_or_else(|| unreachable!("selected woodworking path satisfies board demand")),
        attention_ticks,
        attention_delta: i128::from(attention_ticks) - i128::from(metrics.adze_total_attention),
        timber_delta: i128::from(total_timber.milligrams())
            - i128::from(adze_route.project_timber.milligrams()),
        adze_route,
        adze_setup,
        bare_immediate_ticks,
        adze_immediate_ticks,
    }
}

struct WoodworkingReportContext<'a> {
    registries: &'a Registries,
    case: FocusedProbeCase,
    behavior_seed: u64,
    world: &'a WoodworkingWorld,
    demand: WoodworkingDemandPlan,
    decision: WoodworkingDecisionPlan,
    metrics: WoodworkingLifecycleMetrics,
    selected: &'a SelectedWoodworkingRoute,
}

fn woodworking_counterfactual_tradeoff(
    registries: &Registries,
    metrics: WoodworkingLifecycleMetrics,
    adze_timber: Mass,
) -> String {
    match (metrics.saw_total_attention, metrics.saw_total_timber) {
        (Some(saw_attention), Some(saw_timber)) => {
            let attention_delta =
                i128::from(saw_attention) - i128::from(metrics.adze_total_attention);
            let timber_delta =
                i128::from(saw_timber.milligrams()) - i128::from(adze_timber.milligrams());
            format!(
                "attention:{attention_delta:+}t/{} timber:{timber_delta:+}mg",
                signed_physical_duration(registries, attention_delta)
            )
        }
        _ => "unavailable:copper".to_owned(),
    }
}

fn report_woodworking_experience(context: &WoodworkingReportContext<'_>) {
    let WoodworkingReportContext {
        registries,
        case,
        behavior_seed,
        world,
        demand,
        decision,
        metrics,
        selected,
    } = context;
    let adze_route_time = format_physical_duration(registries, metrics.adze_total_attention);
    let saw_route_attention = metrics.saw_total_attention.unwrap_or(0);
    let saw_route_time = format_physical_duration(registries, saw_route_attention);
    let selected_setup_time = format_physical_duration(registries, selected.setup_ticks);
    let selected_active_time = format_physical_duration(registries, selected.route.active_ticks());
    let selected_total_time = format_physical_duration(registries, selected.attention_ticks);
    let attention_delta_time = signed_physical_duration(registries, selected.attention_delta);
    let saw_counterfactual_tradeoff = woodworking_counterfactual_tradeoff(
        registries,
        *metrics,
        selected.adze_route.project_timber,
    );
    let selected_condition = selected
        .route
        .final_condition_ppm
        .map_or_else(|| "no-tool".to_owned(), |ppm| format!("{ppm}ppm"));
    let adze_immediate_total = selected
        .adze_setup
        .checked_add(selected.adze_immediate_ticks)
        .unwrap_or_else(|| panic!("woodworking immediate adze attention overflowed"));
    let adze_immediate_total_time = format_physical_duration(registries, adze_immediate_total);
    let bare_immediate_time = format_physical_duration(registries, selected.bare_immediate_ticks);
    let adze_immediate_time = format_physical_duration(registries, selected.adze_immediate_ticks);
    let saw_route_timber = metrics.saw_total_timber.map_or(0, Mass::milligrams);
    reviewln!(
        "WOODWORKING EXPERIENCE seed=0x{:016X} behavior=0x{behavior_seed:016X} sample={} demand-horizon={} demand=[immediate:{}mg queued:{}mg pipeline:{}mg boards] preference={} policy-basis=pre-action-budget-not-lifecycle-oracle copper-counterfactual=[available:{}mg blade:{}mg protected-reserve:{}mg lifecycle-spend:{}mg after-saw:{}mg] routes=[adze:{}logs timber:{}mg attention:{}t/{adze_route_time} production:{}t maintenance:{}t/{}services final-condition:{}ppm; saw-assisted:min-saw-logs:{} fundable:{} setup-timber:{}mg actual=[saw:{} adze-fallback:{} fallback-copper:{} saw-services:{} adze-services:{}] timber:{}mg attention:{}t/{saw_route_time} attention-payback:{} timber-saving:{} timber-neutral:{} counterfactual-vs-adze=[{saw_counterfactual_tradeoff}]] choice={} reason={} selected=[setup:{}t/{selected_setup_time} active:{}t/{selected_active_time} total:{}t/{selected_total_time} timber:{}mg project-timber:{}mg boards:{}mg surplus:{}mg chips:{}mg condition:{selected_condition}] selected-vs-adze=[attention:{:+}t/{attention_delta_time} timber:{:+}mg] immediate-baseline=[bare:{}t/{bare_immediate_time} adze:{adze_immediate_total}t/{adze_immediate_total_time} adze-work-only:{}t/{adze_immediate_time}] matter=conserved",
        case.seed(),
        focused_probe_role_label(case.role()),
        demand.horizon,
        demand.immediate_boards.milligrams(),
        demand
            .pipeline_boards
            .checked_sub(demand.immediate_boards)
            .unwrap_or_else(|| unreachable!("pipeline demand includes immediate demand"))
            .milligrams(),
        demand.pipeline_boards.milligrams(),
        decision.preference.label(),
        world.copper_available.milligrams(),
        world.blade_input.milligrams(),
        world.protected_copper_reserve.milligrams(),
        metrics.saw_copper_consumed.milligrams(),
        metrics.copper_after_saw.milligrams(),
        demand.adze_batches,
        selected.adze_route.project_timber.milligrams(),
        metrics.adze_total_attention,
        selected.adze_route.production_ticks,
        selected.adze_route.maintenance_ticks,
        selected.adze_route.maintenance_services,
        selected.adze_route.final_condition_ppm.unwrap_or(0),
        demand.saw_batches,
        world.saw_fundable,
        metrics.saw_setup_timber,
        metrics.saw_actual_batches,
        metrics.saw_fallback_adze_batches,
        metrics.saw_fallback_due_to_copper,
        metrics.saw_service_count,
        metrics.saw_fallback_adze_service_count,
        saw_route_timber,
        saw_route_attention,
        metrics.saw_attention_payback,
        metrics.saw_timber_saving,
        metrics.saw_timber_neutral,
        selected.choice,
        decision.reason.label(),
        selected.setup_ticks,
        selected.route.active_ticks(),
        selected.attention_ticks,
        selected.total_timber.milligrams(),
        selected.route.project_timber.milligrams(),
        selected.boards.milligrams(),
        selected.board_surplus.milligrams(),
        selected.chips.milligrams(),
        selected.attention_delta,
        selected.timber_delta,
        selected.bare_immediate_ticks,
        selected.adze_immediate_ticks,
    );
}

fn report_woodworking_result(
    context: WoodworkingReportContext<'_>,
) -> (&'static str, u64, Option<u64>) {
    let bare_time = format_physical_duration(context.registries, context.decision.bare_attention);
    let adze_route_time =
        format_physical_duration(context.registries, context.metrics.adze_total_attention);
    let selected_total_time =
        format_physical_duration(context.registries, context.selected.attention_ticks);
    reviewln!(
        "WOODWORKING BASELINE seed=0x{:016X} bare={}t/{bare_time} adze={}t/{adze_route_time} selected={}t/{selected_total_time} choice={} basis=full-lifecycle-including-tool-construction",
        context.case.seed(),
        context.decision.bare_attention,
        context.metrics.adze_total_attention,
        context.selected.attention_ticks,
        context.selected.choice,
    );
    report_woodworking_experience(&context);
    reviewln!(
        "WOODWORKING FEEDBACK seed=0x{:016X} basis=executed-lifecycle-versus-pre-action-policy-model attention=[setup-budget-met:{} actual-payback:{}] timber=[nominal:{} actual:{}] selected={} choice-revised-after-outcome=false",
        context.case.seed(),
        context.decision.setup_attention_budget_met,
        context.metrics.saw_attention_payback,
        context.decision.nominal_timber_balance.label(),
        context.metrics.actual_timber_balance.label(),
        context.selected.choice,
    );
    (
        context.selected.choice,
        context.selected.attention_ticks,
        context.metrics.saw_total_attention,
    )
}

fn evaluate_woodworking_probe(
    registries: &Registries,
    case: FocusedProbeCase,
) -> (&'static str, u64, Option<u64>) {
    let seed = case.seed();
    let behavior_seed = case
        .behavior_seed()
        .unwrap_or_else(|| panic!("woodworking actor case lost its independent behavior seed"));
    let demand = plan_woodworking_demand(registries, seed);
    let world = build_woodworking_world(registries, seed);
    let decision = plan_woodworking_investment(registries, behavior_seed, demand, &world);
    reviewln!(
        "WOODWORKING DECISION seed=0x{seed:016X} basis=pre-action-inventory+authored-routes budget=bare-work-at-least-twice-hand-build bare-work={}t adze-build-budget={}t timber=nominal-no-future-service reserve-safe-now={} saw={} bare={} future-outcomes=diagnostic-only",
        decision.bare_attention,
        decision.adze_budget,
        decision.reserve_safe_now,
        decision.invest_in_saw,
        decision.use_bare_hands,
    );
    let lifecycle = execute_woodworking_lifecycle(registries, &world, demand, decision);
    let metrics = evaluate_woodworking_lifecycle(case, &world, decision, &lifecycle);
    let selected = select_and_validate_woodworking_route(
        WoodworkingSelectionContext {
            registries,
            world: &world,
            demand,
            decision,
            metrics,
        },
        lifecycle,
    );
    report_woodworking_result(WoodworkingReportContext {
        registries,
        case,
        behavior_seed,
        world: &world,
        demand,
        decision,
        metrics,
        selected: &selected,
    })
}

#[cfg(test)]
#[test]
fn woodworking_keeps_pre_action_setup_budget_choice_when_realized_saw_is_cheaper() {
    let registries = deep_hearth::content::build_registries();
    // A finite intermediate order with sufficient copper: the conservative actor will
    // not spend its construction budget even though the completed saw route is cheaper.
    // Reject the saw when pre-action intent cannot fund it.
    let witness = (80..=120).find(|&seed| {
        let (choice, selected_attention, saw_attention) = evaluate_woodworking_probe(
            &registries,
            FocusedProbeCase::new(seed, Some(2), FocusedProbeRole::OrganicVariation),
        );
        choice == "stone-adze" && saw_attention.is_some_and(|ticks| ticks < selected_attention)
    });
    assert!(
        witness.is_some(),
        "bounded organic workloads must retain an adze choice that later saw outcomes cannot rewrite"
    );
}
