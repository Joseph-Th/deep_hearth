//! Fixed attention-return criterion for physical capital investment comparisons.

// Capital should feel like a decisive reduction in future player attention, not a technically
// positive spreadsheet result. Require the disclosed workload to beat break-even by one fifth of
// the candidate's extra setup attention. This still permits project-scale specialization while
// rejecting machinery that consumes substantial material and setup for only a few saved ticks.
// One maintained criterion keeps comparison evidence reproducible; the reviewer's own play decides
// whether a real opportunity is worth taking.
pub(super) const MINIMUM_CAPITAL_RETURN_PPM: u64 = 200_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CapitalInvestmentPolicy;

impl CapitalInvestmentPolicy {
    pub(super) const fn baseline() -> Self {
        Self
    }

    #[allow(
        dead_code,
        reason = "focused targets use the shared return floor without reading the raw criterion"
    )]
    pub(super) const fn minimum_return_ppm(self) -> u64 {
        MINIMUM_CAPITAL_RETURN_PPM
    }

    pub(super) fn minimum_attention_return(
        self,
        incumbent_setup_attention_ticks: u64,
        candidate_setup_attention_ticks: u64,
    ) -> u64 {
        minimum_attention_return_at(
            incumbent_setup_attention_ticks,
            candidate_setup_attention_ticks,
            MINIMUM_CAPITAL_RETURN_PPM,
        )
    }
}

fn minimum_attention_return_at(
    incumbent_setup_attention_ticks: u64,
    candidate_setup_attention_ticks: u64,
    minimum_return_ppm: u64,
) -> u64 {
    let extra_setup =
        candidate_setup_attention_ticks.saturating_sub(incumbent_setup_attention_ticks);
    extra_setup
        .checked_mul(minimum_return_ppm)
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
