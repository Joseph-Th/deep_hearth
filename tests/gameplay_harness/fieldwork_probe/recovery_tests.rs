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
