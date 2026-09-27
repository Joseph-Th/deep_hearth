//! Shared actor policy for attention-return floors on physical capital investment.

// A candidate must beat break-even by a visible fraction of its extra setup cost. This filters
// token wins near projection crossovers without demanding that the setup cost be repaid twice.
const CAPITAL_MINIMUM_RETURN_PPM: u64 = 50_000;

pub(super) fn minimum_attention_return(
    incumbent_setup_attention_ticks: u64,
    candidate_setup_attention_ticks: u64,
) -> u64 {
    let extra_setup =
        candidate_setup_attention_ticks.saturating_sub(incumbent_setup_attention_ticks);
    extra_setup
        .checked_mul(CAPITAL_MINIMUM_RETURN_PPM)
        .unwrap_or_else(|| panic!("capital investment return floor overflowed"))
        .div_ceil(1_000_000)
}

pub(super) fn clears_attention_return(
    incumbent_lifecycle_attention_ticks: u64,
    candidate_lifecycle_attention_ticks: u64,
    minimum_attention_return_ticks: u64,
) -> bool {
    let saving =
        incumbent_lifecycle_attention_ticks.saturating_sub(candidate_lifecycle_attention_ticks);
    saving > 0 && saving >= minimum_attention_return_ticks
}
