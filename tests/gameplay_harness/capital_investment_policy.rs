//! Shared actor policy for attention-return floors on physical capital investment.

// Capital should feel like a decisive reduction in future player attention, not a technically
// positive spreadsheet result. Require the disclosed workload to beat break-even by one fifth of
// the candidate's extra setup attention. This still permits project-scale specialization while
// rejecting machinery that consumes substantial material and setup for only a few saved ticks.
const CAPITAL_MINIMUM_RETURN_PPM: u64 = 200_000;

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
