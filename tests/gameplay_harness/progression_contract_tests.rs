//! Cheap primitive-progression topology and generator contracts.

use std::collections::BTreeSet;

use deep_hearth::content::gameplay_fixture::seed_lot;
use deep_hearth::content::{
    ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE, ENERGY_STONE_FLYWHEEL_DRIVE,
    EQUIPMENT_COPPER_REINFORCED_GEOLOGICAL_HAMMER, EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
    EQUIPMENT_COPPER_REINFORCED_PICK, EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER,
    EQUIPMENT_COPPER_REINFORCED_STONE_QUARRY_PICK, EQUIPMENT_COPPER_REINFORCED_STONE_ROTARY_QUERN,
    EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR, EQUIPMENT_COPPER_REINFORCED_WOODWORKING_ADZE,
    FORM_BOARD, FORM_CHIP, FORM_REINFORCEMENT, FORM_SCRAP, FORM_TOOL, MATERIAL_COPPER,
    MATERIAL_STONE, MATERIAL_WOOD, PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
    PROCESS_COLD_WORK_COPPER_SCRAP_REINFORCEMENT, PROCESS_HAND_BREAK_ORE,
    PROCESS_HAND_SORT_NATIVE_COPPER, PROCESS_KNAP_STONE_TOOL, PROCESS_REKNAP_STONE_SCRAP_TOOL,
    PROCESS_SEPARATE_NATIVE_COPPER, PROCESS_SHAPE_WOOD_BOARDS, build_registries,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::AppState;
use deep_hearth::core::time::WorldSeed;
use deep_hearth::material::CommodityKey;
use deep_hearth::registry::ProcessEquipmentRole;

use super::catalog::{ProcessResolverKind, process_catalog_entries};
use super::environment::ROOM_TEMPERATURE;
use super::focused_seeds::{FocusedProbeCase, FocusedProbeRole};
use super::inventory_support::add_solid_stockpile;
use super::manual_craft_planning::{
    manual_craft_plan_for_available_output, manual_craft_topology_plan_for_output,
};
use super::progression_probe::{
    DEEP_OPPORTUNITY_MIN_BATCHES, MARGINAL_OPPORTUNITY_MAX_BATCHES,
    MARGINAL_OPPORTUNITY_MIN_BATCHES, PrimitivePriority, PrimitiveReinvestmentOutcome,
    PrimitiveSteadyStop, SHALLOW_OPPORTUNITY_MAX_BATCHES,
    manual_processing::manual_processing_setup, ore_opportunity,
    review::evaluate_primitive_progression_probe, varied_four_way_order,
};

#[test]
fn bootstrap_planning_excludes_faster_required_equipment_producers() {
    let registries = build_registries();
    let boards = CommodityKey::new(MATERIAL_WOOD, FORM_BOARD);
    assert!(
        registries
            .crafting()
            .manual_producers(boards)
            .any(|definition| registries
                .process_topology(definition.process())
                .is_some_and(
                    |topology| topology.equipment_role() == ProcessEquipmentRole::Required
                )),
        "bootstrap-planning regression requires a competing required-equipment board route"
    );

    let (selected, batches) = manual_craft_topology_plan_for_output(
        &registries,
        boards,
        Mass::from_milligrams(800_000),
        "bootstrap-planning regression",
    );

    assert_eq!(selected.process(), PROCESS_SHAPE_WOOD_BOARDS);
    assert_eq!(batches, 1);
    assert_ne!(
        registries
            .process_topology(selected.process())
            .unwrap_or_else(|| panic!("selected bootstrap route lost process topology"))
            .equipment_role(),
        ProcessEquipmentRole::Required
    );
}

#[test]
fn current_manual_craft_planning_ignores_unowned_salvage_inputs() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x504C_414E_0001));
    let raw = add_solid_stockpile(&mut state, Mass::from_milligrams(10_000_000));
    seed_lot(
        &registries,
        &mut state,
        raw,
        CommodityKey::new(MATERIAL_WOOD, deep_hearth::content::FORM_LOG),
        Mass::from_milligrams(10_000_000),
        ROOM_TEMPERATURE,
    );
    let boards = CommodityKey::new(MATERIAL_WOOD, FORM_BOARD);

    let (selected, batches, selected_source) = manual_craft_plan_for_available_output(
        &registries,
        &state,
        &[raw],
        boards,
        Mass::from_milligrams(1_600_000),
        "available board-route regression",
    );

    assert_eq!(selected.process(), PROCESS_SHAPE_WOOD_BOARDS);
    assert_eq!(batches, 2);
    assert_eq!(selected_source, raw);
}

#[test]
fn primitive_recovery_and_reinforcement_routes_remain_connected() {
    let registries = build_registries();
    let catalog = process_catalog_entries(&registries);

    let hand_break = registries
        .ore_processing()
        .get_manual_comminution(PROCESS_HAND_BREAK_ORE)
        .unwrap_or_else(|| panic!("manual ore breaking disappeared"));
    let manual_sort = registries
        .ore_processing()
        .get_manual_constituent_separation(PROCESS_HAND_SORT_NATIVE_COPPER)
        .unwrap_or_else(|| panic!("manual native-copper sorting disappeared"));
    let powered_sort = registries
        .ore_processing()
        .get_constituent_separation(PROCESS_SEPARATE_NATIVE_COPPER)
        .unwrap_or_else(|| panic!("powered native-copper separation disappeared"));
    assert_eq!(manual_sort.input_form(), hand_break.output_form());
    assert_eq!(
        manual_sort.input_particle_size_range(),
        hand_break.output_particle_size()
    );
    assert!(manual_sort.target_recovery_ppm() > 0);
    assert!(manual_sort.target_recovery_ppm() < powered_sort.target_recovery_ppm());
    assert!(!manual_sort.max_batch_mass().is_zero());
    assert!(!manual_sort.processing_rate().is_zero());

    for process in [PROCESS_HAND_BREAK_ORE, PROCESS_HAND_SORT_NATIVE_COPPER] {
        let entry = catalog
            .iter()
            .find(|entry| entry.process == process)
            .unwrap_or_else(|| {
                panic!("manual progression process is absent from process topology")
            });
        assert!(matches!(
            entry.resolver,
            ProcessResolverKind::ManualComminution | ProcessResolverKind::ManualSeparation
        ));
        assert_eq!(
            (
                entry.nominal_provider_count,
                entry.compatible_energy_store_count
            ),
            (0, 0)
        );
    }

    let fresh_stone = registries
        .crafting()
        .get_manual(PROCESS_KNAP_STONE_TOOL)
        .unwrap_or_else(|| panic!("fresh stone knapping disappeared"));
    let recycled_stone = registries
        .crafting()
        .get_manual(PROCESS_REKNAP_STONE_SCRAP_TOOL)
        .unwrap_or_else(|| panic!("stone scrap reknapping disappeared"));
    assert_eq!(
        recycled_stone.input(),
        CommodityKey::new(MATERIAL_STONE, FORM_SCRAP)
    );
    assert!(recycled_stone.duration() > fresh_stone.duration());
    let recovered = recycled_stone
        .outputs()
        .iter()
        .filter(|output| {
            matches!(
                output.commodity(),
                commodity
                    if commodity == CommodityKey::new(MATERIAL_STONE, FORM_TOOL)
                        || commodity == CommodityKey::new(MATERIAL_STONE, FORM_CHIP)
            )
        })
        .map(|output| output.mass())
        .try_fold(deep_hearth::core::quantity::Mass::ZERO, |total, mass| {
            total.checked_add(mass)
        })
        .unwrap_or_else(|| panic!("stone recovery output mass overflowed"));
    assert_eq!(recovered, recycled_stone.input_mass());

    let native_work = registries
        .crafting()
        .get_manual(PROCESS_COLD_WORK_COPPER_REINFORCEMENT)
        .unwrap_or_else(|| panic!("native-copper reinforcement work disappeared"));
    let scrap_rework = registries
        .crafting()
        .get_manual(PROCESS_COLD_WORK_COPPER_SCRAP_REINFORCEMENT)
        .unwrap_or_else(|| panic!("copper scrap rework disappeared"));
    assert!(scrap_rework.duration() > native_work.duration());

    let reinforcement = CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT);
    let canonical_inputs = registries
        .equipment()
        .get_equipment(EQUIPMENT_COPPER_REINFORCED_PICK)
        .and_then(|definition| definition.upgrade_profile())
        .unwrap_or_else(|| panic!("reinforced pick lost its additive upgrade route"))
        .additions()
        .inputs();
    let canonical = canonical_inputs
        .iter()
        .find(|input| input.commodity() == reinforcement)
        .unwrap_or_else(|| panic!("reinforced pick lost its canonical copper reinforcement input"));
    assert_eq!(
        canonical_inputs
            .iter()
            .filter(|input| input.commodity() == reinforcement)
            .count(),
        1,
        "reinforced pick must identify one unambiguous canonical copper reinforcement input"
    );

    let compatible_targets = registries
        .equipment()
        .definitions()
        .filter_map(|definition| {
            let inputs = definition.upgrade_profile()?.additions().inputs();
            inputs
                .iter()
                .any(|input| {
                    input.commodity() == canonical.commodity() && input.mass() == canonical.mass()
                })
                .then_some(definition.id())
        })
        .collect::<BTreeSet<_>>();
    let required_targets = BTreeSet::from([
        EQUIPMENT_COPPER_REINFORCED_PICK,
        EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
        EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER,
        EQUIPMENT_COPPER_REINFORCED_STONE_QUARRY_PICK,
        EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR,
        EQUIPMENT_COPPER_REINFORCED_STONE_ROTARY_QUERN,
        EQUIPMENT_COPPER_REINFORCED_GEOLOGICAL_HAMMER,
        EQUIPMENT_COPPER_REINFORCED_WOODWORKING_ADZE,
    ]);
    assert!(required_targets.is_subset(&compatible_targets));
    for target in required_targets {
        let definition = registries
            .equipment()
            .get_equipment(target)
            .unwrap_or_else(|| panic!("required primitive reinforcement target disappeared"));
        let maintenance = definition
            .maintenance_profile()
            .unwrap_or_else(|| panic!("required primitive reinforcement target lost maintenance"));
        assert!(
            maintenance.is_component_replacement(),
            "primitive reinforcement target must localize wear to an embodied replaceable component"
        );
    }

    let base_flywheel = registries
        .energy()
        .get_store(ENERGY_STONE_FLYWHEEL_DRIVE)
        .unwrap_or_else(|| panic!("stone flywheel disappeared"));
    let reinforced_flywheel = registries
        .energy()
        .get_store(ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE)
        .unwrap_or_else(|| panic!("reinforced flywheel disappeared"));
    let upgrade = reinforced_flywheel
        .upgrade_profile()
        .unwrap_or_else(|| panic!("reinforced flywheel lost its additive upgrade route"));
    assert_eq!(upgrade.from(), ENERGY_STONE_FLYWHEEL_DRIVE);
    assert_eq!(
        upgrade.additions().inputs(),
        std::slice::from_ref(canonical)
    );
    assert_eq!(base_flywheel.carrier(), reinforced_flywheel.carrier());
    assert!(reinforced_flywheel.capacity() > base_flywheel.capacity());
    assert!(reinforced_flywheel.max_input_power() >= base_flywheel.max_input_power());
    assert!(reinforced_flywheel.max_output_power() >= base_flywheel.max_output_power());
}

#[test]
fn autonomous_crushing_does_not_fill_idle_time_with_unbounded_feed_mining() {
    let registries = build_registries();
    let review = evaluate_primitive_progression_probe(
        &registries,
        FocusedProbeCase::new(0xD33F_C01D_5052, None, FocusedProbeRole::MaintainedAnchor),
    );
    assert!(
        review.steady_feed_buffer_limited_cycles > 0,
        "the actor must stop replenishing a two-batch feed buffer instead of mining solely to occupy machine time"
    );
    assert_eq!(
        review.overlap_setup_equivalent_cycles, None,
        "bounded feed replenishment must not masquerade as economic setup payback"
    );
}

#[test]
fn completed_reinvestment_consumes_post_order_stockpile_for_upgrade_demand() {
    let registries = build_registries();
    let review = evaluate_primitive_progression_probe(
        &registries,
        FocusedProbeCase::new(0xD33F_C01D_5052, None, FocusedProbeRole::MaintainedAnchor),
    );
    assert_eq!(review.steady_state_cycles, 12);
    assert_eq!(
        review.steady_state_stop,
        PrimitiveSteadyStop::StockpileOrderComplete
    );
    assert_eq!(review.overlap_setup_equivalent_cycles, None);
    let PrimitiveReinvestmentOutcome::Completed(work) = review.stockpiling_reinvestment else {
        panic!("maintained post-order stockpile must support executed upgrade demand");
    };
    assert!(work.stockpile_demand_executed);
    assert_eq!(work.stockpile_demand_copper, Mass::from_milligrams(40_000));
    assert_eq!(
        work.stockpile_before_demand
            .checked_sub(work.stockpile_after_demand),
        Some(work.stockpile_demand_feed)
    );
    assert!(!work.stockpile_demand_feed.is_zero());
    assert!(!work.stockpile_demand_energy.is_zero());
    assert!(work.stockpile_demand_separation_ticks > 0);
    let PrimitiveReinvestmentOutcome::Completed(immediate) = review.reinvestment else {
        panic!("the already-owned buffer must fund the same goal without speculative stockpiling");
    };
    assert!(immediate.stockpile_before_demand < work.stockpile_before_demand);
    assert_eq!(
        immediate.stockpile_demand_copper,
        work.stockpile_demand_copper
    );
    assert!(immediate.elapsed_ticks < review.stockpiling_delay_ticks + work.elapsed_ticks);
    let selected = review.selected_end;
    assert_eq!(
        selected.completed_at - selected.decision_at,
        immediate.elapsed_ticks
    );
    assert!(
        selected.crusher_reinforced && selected.separator_reinforced && selected.drive_reinforced
    );
    assert!(
        selected.pick_condition_ppm < 1_000_000,
        "primary state must not receive coverage-only pick replacement"
    );
    assert!(selected.crushed_mass < work.stockpile_after_demand);
    assert!(!selected.native_copper.is_zero());
    assert!(selected.metabolic_energy_spent_nj > 0 && selected.hydration_spent_ul > 0);
}

#[test]
fn bounded_stockpiling_preserves_shallow_supply_until_reinvestment() {
    let registries = build_registries();
    let case = FocusedProbeCase::new(
        11,
        Some(0xE242_49A0_7762_6A70),
        FocusedProbeRole::ExplicitReplay,
    );
    assert!(
        ore_opportunity(case.seed(), false).batch_budget() <= SHALLOW_OPPORTUNITY_MAX_BATCHES,
        "shallow-opportunity regression seed no longer exercises the intended narrow geological reserve"
    );
    let review = evaluate_primitive_progression_probe(&registries, case);
    assert_eq!(review.overlap_setup_equivalent_cycles, None);
    assert!(
        review.steady_state_cycles > 0,
        "shallow opportunity should support the bounded stockpiling order before the later reinvestment exhausts it"
    );
    assert_eq!(
        review.steady_state_stop,
        PrimitiveSteadyStop::StockpileOrderComplete
    );
    assert_eq!(
        review.stockpiling_reinvestment,
        PrimitiveReinvestmentOutcome::TargetSupplyLimited
    );
}

#[test]
fn local_pick_first_sequence_is_measured_against_the_crank_counterfactual() {
    let registries = build_registries();
    let review = evaluate_primitive_progression_probe(
        &registries,
        FocusedProbeCase::new(404, Some(1_648), FocusedProbeRole::ExplicitReplay),
    );
    assert_eq!(review.natural_priority, PrimitivePriority::PickFirst);
    assert!(review.extraction_hard_access_lead_ticks > 0);
    assert!(review.extraction_hard_material_window_ticks > 0);
    assert!(review.mechanization_processed_output_window_ticks > 0);
    assert!(
        review.extraction_hard_access_lead_ticks
            > review.mechanization_processed_output_window_ticks,
        "the local pick-vs-crank state must not be advertised as reciprocal while pick-first buys substantially more immediate player-visible leverage"
    );
}

#[test]
fn progression_generators_cover_distinct_search_and_economic_pressures() {
    let clue_orders = (1_u64..=12)
        .map(varied_four_way_order)
        .collect::<BTreeSet<_>>();
    assert!(
        clue_orders.len() > 1,
        "progression clue ordering collapsed to one permutation"
    );
    assert!(clue_orders.iter().all(|order| {
        let mut sorted = *order;
        sorted.sort_unstable();
        sorted == [0, 1, 2, 3]
    }));

    let opportunities = (1_u64..=32)
        .map(|seed| ore_opportunity(seed, false))
        .collect::<Vec<_>>();
    assert!(
        opportunities
            .iter()
            .any(|opportunity| opportunity.batch_budget() <= SHALLOW_OPPORTUNITY_MAX_BATCHES),
        "organic progression generated no shallow finite opportunity"
    );
    assert!(
        opportunities.iter().any(|opportunity| {
            (MARGINAL_OPPORTUNITY_MIN_BATCHES..=MARGINAL_OPPORTUNITY_MAX_BATCHES)
                .contains(&opportunity.batch_budget())
        }),
        "organic progression generated no marginal finite opportunity"
    );
    assert!(
        opportunities
            .iter()
            .any(|opportunity| opportunity.batch_budget() >= DEEP_OPPORTUNITY_MIN_BATCHES),
        "organic progression generated no deep finite opportunity"
    );
    assert!(
        ore_opportunity(1, true).batch_budget() >= DEEP_OPPORTUNITY_MIN_BATCHES,
        "maintained progression must keep a deep reinvestment opportunity"
    );

    let registries = build_registries();
    let manual_setups = (1_u64..=16)
        .map(|seed| manual_processing_setup(&registries, seed))
        .collect::<Vec<_>>();
    assert!(
        manual_setups
            .iter()
            .map(|setup| setup.ore_mass.milligrams())
            .collect::<BTreeSet<_>>()
            .len()
            > 1
    );
    assert!(
        manual_setups
            .iter()
            .map(|setup| setup.copper_ppm)
            .collect::<BTreeSet<_>>()
            .len()
            > 1
    );
    assert!(
        manual_setups
            .iter()
            .map(|setup| setup.clay_share_ppm)
            .collect::<BTreeSet<_>>()
            .len()
            > 1
    );
}
