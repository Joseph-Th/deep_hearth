//! Episode-level primitive-progression regressions that require the full play-like evaluator.

use std::collections::BTreeSet;

use deep_hearth::content::build_registries;

use super::focused_case::{FocusedProbeCase, FocusedProbeRole};
use super::focused_witnesses::PROGRESSION_MAINTAINED_ANCHOR_SEED;
use super::progression_probe::{
    DEEP_OPPORTUNITY_MIN_BATCHES, MARGINAL_OPPORTUNITY_MAX_BATCHES,
    MARGINAL_OPPORTUNITY_MIN_BATCHES, PrimitivePriority, PrimitiveReinvestmentOutcome,
    PrimitiveSteadyStop, SHALLOW_OPPORTUNITY_MAX_BATCHES,
    manual_processing::manual_processing_setup, ore_opportunity,
    review::evaluate_primitive_progression_probe, varied_four_way_order,
};
use super::seed::mix64;

#[test]
fn organic_progression_worlds_cross_the_live_first_copper_decision_frontier() {
    let registries = build_registries();
    let priorities = (0_u64..4)
        .map(|seed| {
            evaluate_primitive_progression_probe(
                &registries,
                FocusedProbeCase::new(
                    seed,
                    Some(mix64(seed ^ 0x4F52_4741_4E49_435F)),
                    FocusedProbeRole::OrganicVariation,
                ),
            )
            .natural_priority
        })
        .collect::<Vec<_>>();
    assert!(
        priorities.contains(&PrimitivePriority::PickFirst)
            && priorities.contains(&PrimitivePriority::CrankFirst),
        "the four organic world strata must exercise both evidence-driven scarce-copper choices: {priorities:?}"
    );
}

#[test]
fn autonomous_crushing_does_not_fill_idle_time_with_unbounded_feed_mining() {
    let registries = build_registries();
    let review = evaluate_primitive_progression_probe(
        &registries,
        FocusedProbeCase::new(
            PROGRESSION_MAINTAINED_ANCHOR_SEED,
            None,
            FocusedProbeRole::MaintainedAnchor,
        ),
    );
    assert!(
        review.steady_feed_buffer_ready_cycles > 0,
        "the actor must stop replenishing a two-batch feed buffer instead of mining solely to occupy machine time"
    );
    assert_eq!(
        review.steady_feed_buffer_capacity_cycles, 0,
        "the maintained autonomy route must distinguish a successfully prepared next-cycle buffer from a storage-capacity blocker"
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
        FocusedProbeCase::new(
            PROGRESSION_MAINTAINED_ANCHOR_SEED,
            None,
            FocusedProbeRole::MaintainedAnchor,
        ),
    );
    assert!(
        review.steady_state_cycles > 0,
        "post-order reinvestment regression requires completed stockpiling work"
    );
    assert_eq!(
        review.steady_state_stop,
        PrimitiveSteadyStop::StockpileOrderComplete
    );
    assert_eq!(review.overlap_setup_equivalent_cycles, None);
    let PrimitiveReinvestmentOutcome::Completed(work) = review.stockpiling_reinvestment else {
        panic!("maintained post-order stockpile must support executed upgrade demand");
    };
    assert!(work.stockpile_demand_executed);
    assert!(!work.stockpile_demand_copper.is_zero());
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
fn local_first_copper_sequence_is_chosen_from_acquired_grade_evidence() {
    let registries = build_registries();
    for (seed, expected) in [
        (
            PROGRESSION_MAINTAINED_ANCHOR_SEED,
            PrimitivePriority::PickFirst,
        ),
        (3, PrimitivePriority::CrankFirst),
    ] {
        let review = evaluate_primitive_progression_probe(
            &registries,
            FocusedProbeCase::new(seed, Some(1_648), FocusedProbeRole::ExplicitReplay),
        );
        assert_eq!(review.natural_priority, expected);
        assert!(review.extraction_hard_access_lead_ticks > 0);
        assert!(review.extraction_hard_material_window_ticks > 0);
        assert!(review.mechanization_processed_output_window_ticks > 0);
    }
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
