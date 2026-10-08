//! Episode-level primitive-progression regressions that require the full play-like evaluator.

use deep_hearth::content::build_registries;

use super::focused_case::{FocusedProbeCase, FocusedProbeRole};
use super::focused_witnesses::PROGRESSION_MAINTAINED_ANCHOR_SEED;
use super::progression_generation::{SHALLOW_OPPORTUNITY_MAX_BATCHES, ore_opportunity};
use super::progression_probe::{
    PrimitivePriority, PrimitiveReinvestmentOutcome, PrimitiveSteadyStop,
    review::evaluate_primitive_progression_probe,
};
use super::seed::mix64;

#[test]
fn organic_progression_worlds_follow_acquired_grade_evidence() {
    let registries = build_registries();
    for seed in 0_u64..32 {
        let review = evaluate_primitive_progression_probe(
            &registries,
            FocusedProbeCase::new(
                seed,
                Some(mix64(seed ^ 0x4F52_4741_4E49_435F)),
                FocusedProbeRole::OrganicVariation,
            ),
        );
        let expected = if review.hard_ore_evidence_lower_ppm > review.bulk_sample_copper_ppm {
            PrimitivePriority::PickFirst
        } else {
            PrimitivePriority::CrankFirst
        };
        assert_eq!(
            review.natural_priority, expected,
            "scarce-copper sequencing must follow acquired hard-seam evidence versus the exact owned bulk grade"
        );
    }
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
