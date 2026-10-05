//! Builds woodworking actor decisions, matched lifecycle evidence, and replayable report outcomes.

use super::execution::{
    AdzePipelinePlan, SawPipelinePlan, SawSetup, WoodworkingRouteOutcome, assemble_adze,
    assemble_saw, authored_output_mass, checked_mass_times, execute_adze_pipeline,
    execute_bare_pipeline, execute_saw_pipeline, project_saw_setup_budget, projected_board_mass,
    reinforce_adze,
};
use super::*;

#[cfg(not(test))]
#[path = "evaluation/report.rs"]
mod report;

pub(super) fn run_woodworking_probe(registries: &Registries, case: FocusedProbeCase) {
    evaluate_woodworking_probe(registries, case);
}

#[derive(Clone, Copy)]
struct WoodworkingDemandPlan {
    #[cfg_attr(test, allow(dead_code, reason = "exploratory report label"))]
    horizon: &'static str,
    immediate_scale: u64,
    immediate_boards: Mass,
    pipeline_boards: Mass,
    adze_batches: u64,
    saw_batches: u64,
    adze_input_mass: Mass,
    saw_input_mass: Mass,
}

fn plan_woodworking_demand(
    registries: &Registries,
    seed: u64,
    stratified: bool,
) -> WoodworkingDemandPlan {
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
    let (horizon, immediate_scale, queued_scale) = if stratified {
        // Low world-seed bits are a disclosed workload stratum, not an expected route. A four-case
        // exploratory sample therefore spans genuinely small work, a short queue, and two project
        // scales while the remaining seed entropy still varies the exact order inside each band.
        match seed & 0b11 {
            0 => ("immediate-only", 1 + immediate_roll % 3, 0),
            1 => (
                "short-queue",
                2 + immediate_roll % 4,
                1 + (queued_roll >> 8) % 6,
            ),
            2 => (
                "project",
                3 + immediate_roll % 5,
                16 + (queued_roll >> 8) % 18,
            ),
            _ => (
                "project",
                8 + immediate_roll % 5,
                34 + (queued_roll >> 8) % 17,
            ),
        }
    } else {
        let project_queue = queued_roll % 51;
        match project_queue {
            0..=5 => ("immediate-only", 1 + immediate_roll % 3, 0),
            6..=15 => (
                "short-queue",
                2 + immediate_roll % 4,
                1 + (queued_roll >> 8) % 6,
            ),
            _ => ("project", 3 + immediate_roll % 10, project_queue),
        }
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
    reinforcement_input: Mass,
    protected_copper_reserve: Mass,
}

fn visible_native_copper(world: &WoodworkingWorld) -> Mass {
    world
        .state
        .inventory()
        .get_stockpile(world.raw)
        .map(|stockpile| stockpile.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL)))
        .unwrap_or_else(|| panic!("woodworking raw stockpile disappeared"))
}

fn adze_reinforcement_native_copper(registries: &Registries) -> Mass {
    let upgrade = registries
        .equipment()
        .get_equipment(EQUIPMENT_COPPER_REINFORCED_WOODWORKING_ADZE)
        .and_then(|definition| definition.upgrade_profile())
        .unwrap_or_else(|| panic!("woodworking reinforced adze lost its authored upgrade route"));
    assert_eq!(
        upgrade.from(),
        EQUIPMENT_STONE_WOODWORKING_ADZE,
        "woodworking copper-reserve policy must follow the authored adze upgrade route"
    );
    let reinforcement = CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT);
    let reinforcement_required = upgrade
        .additions()
        .inputs()
        .iter()
        .filter(|input| input.commodity() == reinforcement)
        .try_fold(Mass::ZERO, |total, input| total.checked_add(input.mass()))
        .unwrap_or_else(|| panic!("woodworking adze reinforcement requirement overflowed"));
    assert!(
        !reinforcement_required.is_zero(),
        "woodworking reinforced adze must consume authored copper reinforcement"
    );
    let reinforcement_craft = registries
        .crafting()
        .get_manual(PROCESS_COLD_WORK_COPPER_REINFORCEMENT)
        .unwrap_or_else(|| panic!("woodworking copper reinforcement route disappeared"));
    assert_eq!(
        reinforcement_craft.input(),
        CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
        "woodworking reserve is measured in the native copper available to this episode"
    );
    let reinforcement_per_batch = authored_output_mass(reinforcement_craft, reinforcement);
    let batches = reinforcement_required
        .milligrams()
        .div_ceil(reinforcement_per_batch.milligrams());
    checked_mass_times(
        reinforcement_craft.input_mass(),
        batches,
        "future adze reinforcement reserve",
    )
}

fn protected_future_copper_reserve(registries: &Registries) -> Mass {
    checked_mass_times(
        adze_reinforcement_native_copper(registries),
        2,
        "two future adze reinforcement reserves",
    )
}

fn build_woodworking_world(
    registries: &Registries,
    seed: u64,
    stratified: bool,
) -> WoodworkingWorld {
    let blade_input = registries
        .crafting()
        .get_manual(PROCESS_COLD_WORK_COPPER_SAW_BLADE)
        .map(|definition| definition.input_mass())
        .unwrap_or_else(|| panic!("woodworking saw-blade process disappeared"));
    let reinforcement_input = adze_reinforcement_native_copper(registries);
    let protected_copper_reserve = protected_future_copper_reserve(registries);
    let just_reserve_safe = blade_input
        .checked_add(protected_copper_reserve)
        .unwrap_or_else(|| panic!("woodworking saw plus reserve opportunity overflowed"));
    let below_blade = blade_input
        .checked_sub(Mass::from_milligrams(1))
        .unwrap_or(Mass::ZERO);
    let copper_available = if stratified {
        // Couple the same four-case sample only to coarse, actor-visible copper pressure. This keeps
        // small exploratory reports from accidentally presenting four identical funded workshops;
        // policy remains independently seeded and the selected route still comes from live economics.
        match seed & 0b11 {
            0 => below_blade,
            1 => blade_input,
            2 => just_reserve_safe,
            _ => checked_mass_times(just_reserve_safe, 2, "abundant copper opportunity"),
        }
    } else {
        match mix64(seed ^ 0x574F_4F44_434F_5050) % 4 {
            0 => below_blade,
            1 => blade_input,
            2 => just_reserve_safe,
            _ => checked_mass_times(just_reserve_safe, 2, "abundant copper opportunity"),
        }
    };
    let stone_supply = Mass::from_milligrams(5_000_000);
    let wood_supply = Mass::from_milligrams(75_000_000);
    let raw_capacity = stone_supply
        .checked_add(wood_supply)
        .and_then(|mass| mass.checked_add(copper_available))
        .unwrap_or_else(|| panic!("woodworking disclosed raw supply overflowed"));
    let mut state = AppState::new();
    let raw = add_solid_stockpile(&mut state, raw_capacity);
    seed_lot(
        registries,
        &mut state,
        raw,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        stone_supply,
        ROOM_TEMPERATURE,
    );
    seed_lot(
        registries,
        &mut state,
        raw,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        wood_supply,
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
    // These workshop buffers are scenario plumbing rather than the decision under test. Size them
    // from the disclosed finite raw opportunity so content tuning cannot turn a stale fixture
    // capacity into an artificial crafting or maintenance blocker.
    let adze_parts = add_solid_stockpile(&mut state, raw_capacity);
    let saw_parts = add_solid_stockpile(&mut state, raw_capacity);
    let output = add_solid_stockpile(&mut state, raw_capacity);
    let adze_replacement = add_solid_stockpile(&mut state, raw_capacity);
    let adze_spent = add_solid_stockpile(&mut state, raw_capacity);
    let saw_replacement = add_solid_stockpile(&mut state, raw_capacity);
    let saw_spent = add_solid_stockpile(&mut state, raw_capacity);
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("woodworking initial matter audit failed: {error}"))
        .total();
    super::super::world_admission::admit_stationary_player(
        registries,
        &mut state,
        &[
            raw,
            adze_parts,
            saw_parts,
            output,
            adze_replacement,
            adze_spent,
            saw_replacement,
            saw_spent,
        ],
        &[],
        "woodworking investment",
    );
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
        reinforcement_input,
        protected_copper_reserve,
    }
}

#[derive(Clone, Copy)]
struct WoodworkingDecisionPlan {
    preference: WoodworkingInvestmentPreference,
    bare_attention: u64,
    bare_projected_boards: Mass,
    #[cfg_attr(test, allow(dead_code, reason = "exploratory report metric"))]
    adze_budget: u64,
    #[cfg_attr(test, allow(dead_code, reason = "exploratory report metric"))]
    reinforced_adze_budget: Option<u64>,
    #[cfg_attr(test, allow(dead_code, reason = "exploratory report metric"))]
    reinforced_projected_attention: Option<u64>,
    stone_projected_attention: u64,
    saw_budget: Option<(u64, Mass)>,
    reserve_safe_now: bool,
    #[cfg_attr(test, allow(dead_code, reason = "exploratory report metric"))]
    reinforced_reserve_safe_now: bool,
    #[cfg_attr(test, allow(dead_code, reason = "exploratory report metric"))]
    setup_attention_budget_met: bool,
    #[cfg_attr(test, allow(dead_code, reason = "exploratory report metric"))]
    nominal_timber_balance: WoodworkingTimberBalance,
    invest_in_saw: bool,
    invest_in_reinforced_adze: bool,
    use_bare_hands: bool,
    reason: WoodworkingInvestmentReason,
}

/// Prices an adze route the way the lived route will consume attention.
///
/// This is actor-side investment policy, not a second legality path: every productive batch uses
/// the production-owned equipment projection, and service timing/material preparation follows the
/// same authored maintenance threshold/profile used by canonical execution below. Keeping the
/// condition stateful matters because a nominal pristine rate can otherwise make a long queue look
/// substantially cheaper than the tool actually is once service enters the lifecycle.
fn project_adze_lifecycle_attention(
    registries: &Registries,
    state: &AppState,
    raw: StockpileId,
    equipment: deep_hearth::equipment::EquipmentDefinitionId,
    setup_ticks: u64,
    batches: u64,
) -> u64 {
    let definition = registries
        .equipment()
        .get_equipment(equipment)
        .unwrap_or_else(|| panic!("woodworking projected adze definition disappeared"));
    let maintenance = definition
        .maintenance_profile()
        .unwrap_or_else(|| panic!("woodworking projected adze lost maintenance"));
    let thresholds = definition.maintenance_thresholds();
    let replacement_duration = resolve_manual_craft(
        registries,
        state,
        &select_manual_craft_request(
            registries,
            state,
            PROCESS_KNAP_STONE_TOOL,
            raw,
            1,
            "woodworking maintenance projection",
        ),
    )
    .unwrap_or_else(|error| panic!("woodworking maintenance projection failed: {error}"))
    .duration()
    .value();
    let one_batch = NonZeroU64::new(1).unwrap_or_else(|| unreachable!("one is nonzero"));
    let mut condition = Condition::PRISTINE;
    let mut attention = setup_ticks;
    for _ in 0..batches {
        if thresholds.classify(condition) == MaintenanceBand::Critical {
            attention = attention
                .checked_add(replacement_duration)
                .and_then(|ticks| {
                    ticks.checked_add(maintenance.required_service_duration(condition).value())
                })
                .unwrap_or_else(|| panic!("woodworking projected maintenance overflowed"));
            condition = maintenance.restored_condition();
        }
        let projection = project_manual_craft_equipment(
            registries,
            PROCESS_SHAPE_WOOD_BOARDS,
            one_batch,
            equipment,
            condition,
        )
        .unwrap_or_else(|error| panic!("woodworking adze lifecycle projection failed: {error}"));
        attention = attention
            .checked_add(projection.duration().value())
            .unwrap_or_else(|| panic!("woodworking projected lifecycle overflowed"));
        condition = projection.condition_after();
    }
    attention
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
    let copper_available = visible_native_copper(world);
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
    let reinforced_adze_fundable = copper_available >= world.reinforcement_input;
    let reinforced_adze_budget = reinforced_adze_fundable.then(|| {
        project_woodworking_construction_budget(
            registries,
            &world.state,
            world.raw,
            EQUIPMENT_COPPER_REINFORCED_WOODWORKING_ADZE,
        )
        .0
    });
    let stone_projected_attention = project_adze_lifecycle_attention(
        registries,
        &world.state,
        world.raw,
        EQUIPMENT_STONE_WOODWORKING_ADZE,
        adze_budget,
        demand.adze_batches,
    );
    let reinforced_projected_attention = reinforced_adze_budget.map(|setup| {
        project_adze_lifecycle_attention(
            registries,
            &world.state,
            world.raw,
            EQUIPMENT_COPPER_REINFORCED_WOODWORKING_ADZE,
            setup,
            demand.adze_batches,
        )
    });
    let saw_budget = (copper_available >= world.blade_input)
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
    let reserve_safe_now = copper_available
        .checked_sub(world.blade_input)
        .is_some_and(|remaining| remaining >= world.protected_copper_reserve);
    let reinforced_reserve_safe_now = copper_available
        .checked_sub(world.reinforcement_input)
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
    let (mut invest_in_saw, saw_reason) = woodworking_investment_decision(
        preference,
        reserve_safe_now,
        setup_attention_budget_met,
        nominal_timber_balance,
    );
    let reinforced_pays_back = reinforced_projected_attention
        .is_some_and(|ticks| ticks < stone_projected_attention && ticks < bare_attention);
    let invest_in_reinforced_adze = reinforced_pays_back
        && match preference {
            WoodworkingInvestmentPreference::ConserveScarceCopper => reinforced_reserve_safe_now,
            WoodworkingInvestmentPreference::ConserveTimber => !invest_in_saw,
        };
    if preference == WoodworkingInvestmentPreference::ConserveScarceCopper
        && invest_in_reinforced_adze
    {
        invest_in_saw = false;
    }
    let use_bare_hands = !invest_in_saw
        && !invest_in_reinforced_adze
        && bare_attention
            < adze_budget
                .checked_mul(2)
                .unwrap_or_else(|| panic!("adze willingness budget fits"));
    WoodworkingDecisionPlan {
        preference,
        bare_attention,
        bare_projected_boards: projected_board_mass(&bare_pipeline_projection),
        adze_budget,
        reinforced_adze_budget,
        reinforced_projected_attention,
        stone_projected_attention,
        saw_budget,
        reserve_safe_now,
        reinforced_reserve_safe_now,
        setup_attention_budget_met,
        nominal_timber_balance,
        invest_in_saw,
        invest_in_reinforced_adze,
        use_bare_hands,
        reason: if use_bare_hands {
            WoodworkingInvestmentReason::BareHandsAvoidsInvestmentCost
        } else if invest_in_reinforced_adze {
            match preference {
                WoodworkingInvestmentPreference::ConserveScarceCopper => {
                    WoodworkingInvestmentReason::ReinforcedAdzePreservesCopper
                }
                WoodworkingInvestmentPreference::ConserveTimber => {
                    WoodworkingInvestmentReason::ReinforcedAdzeRepaysAttention
                }
            }
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

struct ReinforcedAdzeCounterfactual {
    state: AppState,
    route: WoodworkingRouteOutcome,
    setup_ticks: u64,
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
    reinforced_adze: Option<ReinforcedAdzeCounterfactual>,
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
    decision.saw_budget.map(|_| {
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

    let reinforced_adze = (visible_native_copper(world) >= world.reinforcement_input).then(|| {
        let mut state = common_state.clone();
        let reinforcement_ticks =
            reinforce_adze(registries, &mut state, world.raw, world.adze_parts, adze);
        let setup_ticks = adze_setup
            .checked_add(reinforcement_ticks)
            .unwrap_or_else(|| panic!("woodworking reinforced-adze setup overflowed"));
        assert_eq!(
            Some(setup_ticks),
            decision.reinforced_adze_budget,
            "pre-action reinforced-adze setup projection must match canonical execution"
        );
        let route = execute_adze_pipeline(
            registries,
            &mut state,
            AdzePipelinePlan {
                raw: world.raw,
                output: world.output,
                replacement: world.adze_replacement,
                spent: world.adze_spent,
                adze,
                batches: demand.adze_batches,
            },
        );
        assert!(route.boards >= demand.pipeline_boards);
        assert_eq!(
            route.boards.checked_add(route.chips),
            Some(route.project_timber)
        );
        validate_loaded_state(registries, &state).unwrap_or_else(|error| {
            panic!("woodworking reinforced-adze counterfactual state invalid: {error}")
        });
        ReinforcedAdzeCounterfactual {
            state,
            route,
            setup_ticks,
        }
    });

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
        reinforced_adze,
        saw,
    }
}

#[derive(Clone, Copy)]
struct WoodworkingLifecycleMetrics {
    adze_total_attention: u64,
    #[cfg_attr(test, allow(dead_code, reason = "exploratory report metric"))]
    reinforced_adze_total_attention: Option<u64>,
    #[cfg_attr(test, allow(dead_code, reason = "exploratory report metric"))]
    reinforced_adze_copper_consumed: Mass,
    #[cfg_attr(test, allow(dead_code, reason = "exploratory report metric"))]
    saw_total_timber: Option<Mass>,
    saw_total_attention: Option<u64>,
    #[cfg_attr(test, allow(dead_code, reason = "exploratory report metric"))]
    saw_setup_timber: u64,
    #[cfg_attr(test, allow(dead_code, reason = "exploratory report metric"))]
    saw_actual_batches: u64,
    #[cfg_attr(test, allow(dead_code, reason = "exploratory report metric"))]
    saw_fallback_adze_batches: u64,
    saw_fallback_due_to_copper: bool,
    saw_service_count: u64,
    #[cfg_attr(test, allow(dead_code, reason = "exploratory report metric"))]
    saw_fallback_adze_service_count: u64,
    #[cfg_attr(test, allow(dead_code, reason = "exploratory report metric"))]
    saw_attention_payback: bool,
    #[cfg_attr(test, allow(dead_code, reason = "exploratory report metric"))]
    actual_timber_balance: WoodworkingTimberBalance,
    #[cfg_attr(test, allow(dead_code, reason = "exploratory report metric"))]
    saw_timber_saving: bool,
    #[cfg_attr(test, allow(dead_code, reason = "exploratory report metric"))]
    saw_timber_neutral: bool,
    #[cfg_attr(test, allow(dead_code, reason = "exploratory report metric"))]
    saw_copper_consumed: Mass,
    #[cfg_attr(test, allow(dead_code, reason = "exploratory report metric"))]
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
        (FocusedProbeRole::MaintainedCoverage, 3) => {
            assert_eq!(
                decision.reason,
                WoodworkingInvestmentReason::CopperSupplyLimited
            );
        }
        (FocusedProbeRole::MaintainedCoverage, 12) => {
            assert_eq!(
                decision.reason,
                WoodworkingInvestmentReason::ReinforcedAdzeRepaysAttention
            );
            assert!(
                metrics
                    .reinforced_adze_total_attention
                    .is_some_and(|ticks| ticks < metrics.adze_total_attention)
            );
        }
        (FocusedProbeRole::MaintainedCoverage, 4) => assert_eq!(
            decision.reason,
            WoodworkingInvestmentReason::PipelineTimberNeutralWithinSetupBudget
        ),
        (FocusedProbeRole::MaintainedCoverage, 6) => {
            assert_eq!(
                decision.reason,
                WoodworkingInvestmentReason::ReinforcedAdzePreservesCopper
            );
            assert!(decision.reinforced_reserve_safe_now);
            assert!(
                metrics
                    .reinforced_adze_total_attention
                    .is_some_and(|ticks| ticks < metrics.adze_total_attention)
            );
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
    let initial_copper = visible_native_copper(world);
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
    let reinforced_adze_total_attention = evidence.reinforced_adze.as_ref().map(|reinforced| {
        reinforced
            .setup_ticks
            .checked_add(reinforced.route.active_ticks())
            .unwrap_or_else(|| panic!("woodworking reinforced-adze lifecycle attention overflowed"))
    });
    let copper_after_reinforced =
        evidence
            .reinforced_adze
            .as_ref()
            .map_or(initial_copper, |reinforced| {
                reinforced
                    .state
                    .inventory()
                    .get_stockpile(world.raw)
                    .map(|stockpile| {
                        stockpile.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL))
                    })
                    .unwrap_or_else(|| {
                        panic!("woodworking raw stockpile disappeared after reinforced adze")
                    })
            });
    let reinforced_adze_copper_consumed = initial_copper
        .checked_sub(copper_after_reinforced)
        .unwrap_or_else(|| panic!("woodworking reinforced adze increased available native copper"));
    if evidence.reinforced_adze.is_some() {
        assert_eq!(
            reinforced_adze_copper_consumed, world.reinforcement_input,
            "reinforced adze must consume exactly its authored native-copper preparation"
        );
    }
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
    let expected_saw_copper_consumed = evidence.saw.as_ref().map_or(Mass::ZERO, |saw| {
        checked_mass_times(
            world.blade_input,
            saw.route
                .saw_services
                .checked_add(1)
                .unwrap_or_else(|| panic!("woodworking saw blade count overflowed")),
            "saw lifecycle copper",
        )
    });
    let copper_after_saw = evidence.saw.as_ref().map_or(initial_copper, |saw| {
        saw.state
            .inventory()
            .get_stockpile(world.raw)
            .map(|stockpile| {
                stockpile.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL))
            })
            .unwrap_or_else(|| panic!("woodworking raw stockpile disappeared after saw lifecycle"))
    });
    let saw_copper_consumed = visible_native_copper(world)
        .checked_sub(copper_after_saw)
        .unwrap_or_else(|| panic!("woodworking saw lifecycle increased available native copper"));
    assert_eq!(
        saw_copper_consumed, expected_saw_copper_consumed,
        "executed saw lifecycle copper must match the initial blade plus service replacements"
    );
    let metrics = WoodworkingLifecycleMetrics {
        adze_total_attention,
        reinforced_adze_total_attention,
        reinforced_adze_copper_consumed,
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
        saw_copper_consumed,
        copper_after_saw,
    };
    assert_eq!(
        decision.stone_projected_attention, adze_total_attention,
        "pre-action stone-adze lifecycle projection must match canonical execution"
    );
    if let Some(projected) = decision.reinforced_projected_attention {
        assert_eq!(
            Some(projected),
            reinforced_adze_total_attention,
            "pre-action reinforced-adze lifecycle projection must match canonical execution"
        );
    }
    assert_woodworking_maintained_witness(case, decision, evidence, metrics);
    metrics
}

struct SelectedWoodworkingRoute {
    choice: &'static str,
    #[cfg_attr(test, allow(dead_code, reason = "exploratory report detail"))]
    route: WoodworkingRouteOutcome,
    #[cfg_attr(test, allow(dead_code, reason = "exploratory report detail"))]
    setup_ticks: u64,
    #[cfg_attr(test, allow(dead_code, reason = "exploratory report detail"))]
    total_timber: Mass,
    #[cfg_attr(test, allow(dead_code, reason = "exploratory report detail"))]
    boards: Mass,
    #[cfg_attr(test, allow(dead_code, reason = "exploratory report detail"))]
    chips: Mass,
    #[cfg_attr(test, allow(dead_code, reason = "exploratory report detail"))]
    board_surplus: Mass,
    attention_ticks: u64,
    #[cfg_attr(test, allow(dead_code, reason = "exploratory report detail"))]
    attention_delta: i128,
    #[cfg_attr(test, allow(dead_code, reason = "exploratory report detail"))]
    timber_delta: i128,
    #[cfg_attr(test, allow(dead_code, reason = "exploratory report detail"))]
    adze_route: WoodworkingRouteOutcome,
    #[cfg_attr(test, allow(dead_code, reason = "exploratory report detail"))]
    adze_setup: u64,
    #[cfg_attr(test, allow(dead_code, reason = "exploratory report detail"))]
    bare_immediate_ticks: u64,
    #[cfg_attr(test, allow(dead_code, reason = "exploratory report detail"))]
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
        reinforced_adze,
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
    } else if decision.invest_in_reinforced_adze {
        let reinforced = reinforced_adze
            .unwrap_or_else(|| unreachable!("reinforced-adze choice requires a fundable route"));
        (
            "reinforced-adze",
            reinforced.state,
            reinforced.route,
            reinforced.setup_ticks,
            reinforced.route.project_timber,
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

fn evaluate_woodworking_probe(
    registries: &Registries,
    case: FocusedProbeCase,
) -> (&'static str, u64, Option<u64>) {
    let seed = case.seed();
    let behavior_seed = case.required_behavior_seed("woodworking investment policy");
    let stratified = matches!(
        case.role(),
        FocusedProbeRole::OrganicVariation | FocusedProbeRole::ExplicitReplay
    );
    let demand = plan_woodworking_demand(registries, seed, stratified);
    let world = build_woodworking_world(registries, seed, stratified);
    let decision = plan_woodworking_investment(registries, behavior_seed, demand, &world);
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
    #[cfg(not(test))]
    report::report_woodworking_result(
        registries,
        case,
        behavior_seed,
        &world,
        demand,
        decision,
        metrics,
        &selected,
    );
    (
        selected.choice,
        selected.attention_ticks,
        metrics.saw_total_attention,
    )
}

#[cfg(test)]
include_woodworking_evaluation_contract_tests!();
