//! Replayable settlement demand and investment-policy generation.

use super::capital_investment_policy::CapitalInvestmentPolicy;

pub(super) fn organic_investment_policy(behavior_seed: u64) -> CapitalInvestmentPolicy {
    CapitalInvestmentPolicy::from_behavior_seed(behavior_seed)
}

pub(super) fn organic_lumber_batches(entropy: u64, baseline_crossover_batches: u64) -> u64 {
    let spread = (baseline_crossover_batches / 2).max(1);
    let lower = baseline_crossover_batches.saturating_sub(spread).max(1);
    let upper = baseline_crossover_batches
        .checked_add(spread)
        .unwrap_or_else(|| panic!("settlement organic order range overflowed"));
    lower + entropy % (upper - lower + 1)
}
