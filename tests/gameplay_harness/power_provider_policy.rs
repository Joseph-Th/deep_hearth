//! Pure actor policy for human-power capital investment.

#[path = "capital_investment_policy.rs"]
mod capital;
use capital::{clears_attention_return, minimum_attention_return};

pub(super) fn primitive_treadle_minimum_attention_return(
    crank_setup_attention_ticks: u64,
    treadle_setup_attention_ticks: u64,
) -> u64 {
    minimum_attention_return(crank_setup_attention_ticks, treadle_setup_attention_ticks)
}

pub(super) fn primitive_treadle_clears_attention_return(
    crank_lifecycle_attention_ticks: u64,
    treadle_lifecycle_attention_ticks: u64,
    minimum_attention_return_ticks: u64,
) -> bool {
    clears_attention_return(
        crank_lifecycle_attention_ticks,
        treadle_lifecycle_attention_ticks,
        minimum_attention_return_ticks,
    )
}

pub(super) fn settlement_walking_minimum_attention_return(
    treadle_setup_attention_ticks: u64,
    walking_setup_attention_ticks: u64,
) -> u64 {
    minimum_attention_return(treadle_setup_attention_ticks, walking_setup_attention_ticks)
}

pub(super) fn settlement_walking_clears_attention_return(
    treadle_lifecycle_attention_ticks: u64,
    walking_lifecycle_attention_ticks: u64,
    minimum_attention_return_ticks: u64,
) -> bool {
    clears_attention_return(
        treadle_lifecycle_attention_ticks,
        walking_lifecycle_attention_ticks,
        minimum_attention_return_ticks,
    )
}

#[cfg(test)]
#[path = "power_provider_policy_tests.rs"]
mod tests;
