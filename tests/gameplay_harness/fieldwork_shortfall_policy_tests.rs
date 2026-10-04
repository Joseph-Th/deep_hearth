//! Fast contracts for bounded fieldwork shortfall policy.

use deep_hearth::core::quantity::Mass;

use super::shortfall_policy::{
    InitialShortfallTerminal, demand_sized_followup_sites, shortfall_terminal,
};

#[test]
fn shortfall_survey_horizon_scales_with_visible_remaining_demand_and_caps_at_local_supply() {
    let site_upper = Mass::from_milligrams(1_000_000);
    let available_sites = 6;
    assert_eq!(
        demand_sized_followup_sites(
            Mass::from_milligrams(1),
            site_upper,
            500_000,
            available_sites,
        ),
        2
    );
    assert_eq!(
        demand_sized_followup_sites(site_upper, site_upper, 500_000, available_sites),
        2
    );
    assert_eq!(
        demand_sized_followup_sites(
            Mass::from_milligrams(site_upper.milligrams() + 1),
            site_upper,
            500_000,
            available_sites,
        ),
        4
    );
    assert_eq!(
        demand_sized_followup_sites(
            Mass::from_milligrams(site_upper.milligrams() * 20),
            site_upper,
            500_000,
            available_sites,
        ),
        available_sites
    );
    assert_eq!(
        demand_sized_followup_sites(site_upper, site_upper, 1_000_000, available_sites),
        1,
        "a certain productive-site prior needs no barren-ground allowance"
    );
    assert_eq!(
        demand_sized_followup_sites(site_upper, site_upper, 0, available_sites),
        available_sites,
        "with no expected productive yield, the bounded local search is the only honest horizon"
    );
}

#[test]
fn shortfall_terminal_distinguishes_completion_budget_exhaustion_and_local_exhaustion() {
    let unfinished = Mass::from_milligrams(1);
    assert_eq!(
        shortfall_terminal(Mass::ZERO, 2, 6),
        InitialShortfallTerminal::OrderComplete
    );
    assert_eq!(
        shortfall_terminal(unfinished, 4, 6),
        InitialShortfallTerminal::PlannedSearchHorizonExhausted
    );
    assert_eq!(
        shortfall_terminal(unfinished, 6, 6),
        InitialShortfallTerminal::LocalSearchAreaExhausted
    );
}
