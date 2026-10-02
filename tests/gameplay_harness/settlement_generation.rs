//! Replayable settlement demand and investment-policy generation.

use super::capital_investment_policy::CapitalInvestmentPolicy;

pub(super) fn organic_investment_policy(behavior_seed: u64) -> CapitalInvestmentPolicy {
    CapitalInvestmentPolicy::from_behavior_seed(behavior_seed)
}

pub(super) fn organic_lumber_batches(
    entropy: u64,
    baseline_crossover_batches: Option<u64>,
    opportunity_batches: u64,
) -> u64 {
    assert!(
        opportunity_batches > 1,
        "settlement organic demand needs room for a lived follow-up order"
    );
    let maximum_order = opportunity_batches - 1;
    let Some(crossover) = baseline_crossover_batches else {
        return 1 + entropy % maximum_order;
    };
    let spread = (crossover / 2).max(1);
    let lower = crossover.saturating_sub(spread).max(1).min(maximum_order);
    let upper = crossover
        .checked_add(spread)
        .unwrap_or(u64::MAX)
        .min(maximum_order);
    lower + entropy % (upper - lower + 1)
}
