//! Disclosed inherited survival states used by stationary gameplay episodes.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StationarySurvivalStart {
    FullReserve,
    HungerWarningBoundary,
    HydrationWarningBoundary,
}

impl StationarySurvivalStart {
    #[cfg(not(test))]
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::FullReserve => "full-reserve",
            Self::HungerWarningBoundary => "hunger-warning-boundary",
            Self::HydrationWarningBoundary => "hydration-warning-boundary",
        }
    }
}
