//! Tests strict aggregate future-capacity arithmetic.

use super::{
    checked_combined_demand, checked_demand_after_adjustment, checked_demand_after_release,
};

#[test]
fn aggregate_future_demand_rejects_overflow_instead_of_saturating() {
    assert_eq!(checked_combined_demand(u64::MAX, 1), None);
    assert_eq!(checked_combined_demand(u64::MAX - 1, 1), Some(u64::MAX));
}

#[test]
fn releasing_future_demand_is_exact_and_rejects_overrelease() {
    assert_eq!(checked_demand_after_release(3, 1), Some(2));
    assert_eq!(checked_demand_after_release(1, 1), Some(0));
    assert_eq!(checked_demand_after_release(0, 1), None);
}

#[test]
fn adjusting_future_demand_releases_before_adding_new_obligations() {
    assert_eq!(checked_demand_after_adjustment(3, 2, 1), Some(4));
    assert_eq!(
        checked_demand_after_adjustment(u64::MAX, 1, 1),
        Some(u64::MAX)
    );
    assert_eq!(checked_demand_after_adjustment(0, 1, 1), None);
}
