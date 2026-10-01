//! Pure actor policy for sizing and classifying bounded fieldwork shortfall recovery.

use deep_hearth::core::quantity::Mass;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum InitialShortfallTerminal {
    OrderComplete,
    PlannedSearchHorizonExhausted,
    LocalSearchAreaExhausted,
}

pub(super) fn demand_sized_followup_sites(
    remaining: Mass,
    observed_site_upper: Mass,
    available_sites: u64,
) -> u64 {
    assert!(
        !remaining.is_zero(),
        "fieldwork shortfall horizon requires unfinished demand"
    );
    assert!(
        available_sites > 0,
        "fieldwork shortfall horizon requires at least one candidate site"
    );
    let comparable_site_mass = observed_site_upper.milligrams().max(1);
    remaining
        .milligrams()
        .div_ceil(comparable_site_mass)
        .clamp(1, available_sites)
}

pub(super) fn shortfall_terminal(
    remaining: Mass,
    sites_visited: u64,
    available_sites: u64,
) -> InitialShortfallTerminal {
    if remaining.is_zero() {
        InitialShortfallTerminal::OrderComplete
    } else if sites_visited >= available_sites {
        InitialShortfallTerminal::LocalSearchAreaExhausted
    } else {
        InitialShortfallTerminal::PlannedSearchHorizonExhausted
    }
}
