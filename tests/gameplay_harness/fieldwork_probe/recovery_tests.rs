//! Demand-sized fieldwork shortfall survey-horizon tests.

use super::*;

#[test]
fn shortfall_survey_horizon_scales_with_visible_remaining_demand() {
    assert_eq!(
        demand_sized_followup_sites(
            Mass::from_milligrams(1_590_874),
            Mass::from_milligrams(1_000_000),
            6,
        ),
        2
    );
    assert_eq!(
        demand_sized_followup_sites(
            Mass::from_milligrams(19_741_048),
            Mass::from_milligrams(5_000_000),
            6,
        ),
        4
    );
    assert_eq!(
        demand_sized_followup_sites(
            Mass::from_milligrams(23_390_168),
            Mass::from_milligrams(1_000_000),
            6,
        ),
        6
    );
}

#[test]
fn shortfall_execution_horizon_is_bounded_by_the_actor_plan() {
    let available = u64::try_from(FOLLOWUP_CHANNEL_STARTS.len())
        .unwrap_or_else(|_| unreachable!("bounded follow-up site count fits u64"));
    for planned in 1..=available {
        let site_count = usize::try_from(planned)
            .unwrap_or_else(|_| unreachable!("bounded follow-up horizon fits usize"));
        assert_eq!(
            FOLLOWUP_CHANNEL_STARTS[..site_count].len(),
            site_count,
            "planned follow-up horizon must map one-for-one to executable local sites"
        );
    }
}

#[test]
fn shortfall_terminal_distinguishes_budget_exhaustion_from_local_exhaustion() {
    let unfinished = Mass::from_milligrams(1);
    assert_eq!(shortfall_terminal(Mass::ZERO, 2, 6), "order-complete");
    assert_eq!(
        shortfall_terminal(unfinished, 4, 6),
        "planned-search-horizon-exhausted"
    );
    assert_eq!(
        shortfall_terminal(unfinished, 6, 6),
        "local-search-area-exhausted"
    );
}
