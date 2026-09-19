//! Tests strict aggregate future-capacity arithmetic.

use super::checked_combined_demand;

#[test]
fn aggregate_future_demand_rejects_overflow_instead_of_saturating() {
    assert_eq!(checked_combined_demand(u64::MAX, 1), None);
    assert_eq!(checked_combined_demand(u64::MAX - 1, 1), Some(u64::MAX));
}
