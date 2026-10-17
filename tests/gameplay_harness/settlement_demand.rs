//! Shared physical lumber-demand generation for settlement-continuity gameplay probes.
//!
//! This module owns only disclosed workload generation. It must not depend on machine/provider
//! crossover analysis, actor investment policy, or inherited equipment condition.

pub(crate) const SETTLEMENT_SPECIALIZATION_OPPORTUNITY_BATCHES: u64 = 192;

pub(crate) fn organic_lumber_batch_limits(opportunity_batches: u64) -> (u64, u64) {
    assert!(
        opportunity_batches > 1,
        "settlement organic demand needs room for a lived follow-up order"
    );
    let maximum_order = opportunity_batches - 1;
    let minimum_order = (opportunity_batches / 24).max(1).min(maximum_order);
    let organic_maximum = opportunity_batches
        .checked_mul(3)
        .map(|value| value / 8)
        .unwrap_or(maximum_order)
        .max(minimum_order)
        .min(maximum_order);
    (minimum_order, organic_maximum)
}

pub(crate) fn organic_lumber_batches(
    world_stratum: u64,
    entropy: u64,
    opportunity_batches: u64,
) -> u64 {
    // Organic demand is an input, not a desired investment result. Keep it independent from the
    // live mechanization crossover and spread a bounded sample across a plausible early-settlement
    // project envelope. The investment decision must emerge from current economics.
    let (minimum_order, organic_maximum) = organic_lumber_batch_limits(opportunity_batches);
    let span = organic_maximum - minimum_order + 1;
    let stratum_width = span.div_ceil(4);
    let stratum = world_stratum & 0b11;
    let lower = minimum_order
        .saturating_add(stratum_width.saturating_mul(stratum))
        .min(organic_maximum);
    let upper = lower
        .saturating_add(stratum_width.saturating_sub(1))
        .min(organic_maximum);
    lower + entropy % (upper - lower + 1)
}
