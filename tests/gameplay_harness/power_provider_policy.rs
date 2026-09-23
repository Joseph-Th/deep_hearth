//! Pure actor policy for primitive human-power capital investment.

// Require a visible return on the treadle's extra setup without treating the provider-only
// projection as exact. Ordinary crusher wear fragments long jobs into more charge events than a
// pristine full-buffer estimate, which systematically increases the value of the faster provider.
// Five percent keeps token one-to-five tick wins on the crank while admitting the observed
// crossover edge once the projected saving is large enough to cover that known fragmentation risk.
const PRIMITIVE_TREADLE_MINIMUM_RETURN_PPM: u64 = 50_000;

pub(super) fn primitive_treadle_minimum_attention_return(
    crank_setup_attention_ticks: u64,
    treadle_setup_attention_ticks: u64,
) -> u64 {
    let extra_setup = treadle_setup_attention_ticks.saturating_sub(crank_setup_attention_ticks);
    extra_setup
        .checked_mul(PRIMITIVE_TREADLE_MINIMUM_RETURN_PPM)
        .unwrap_or_else(|| panic!("primitive treadle return floor overflowed"))
        .div_ceil(1_000_000)
}

pub(super) fn primitive_treadle_clears_attention_return(
    crank_lifecycle_attention_ticks: u64,
    treadle_lifecycle_attention_ticks: u64,
    minimum_attention_return_ticks: u64,
) -> bool {
    let saving = crank_lifecycle_attention_ticks.saturating_sub(treadle_lifecycle_attention_ticks);
    saving > 0 && saving >= minimum_attention_return_ticks
}

#[cfg(test)]
#[path = "power_provider_policy_tests.rs"]
mod tests;
