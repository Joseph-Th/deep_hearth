//! Tests for the sibling arithmetic module; isolated so test-only edits do not invalidate production builds.

use super::*;

#[test]
fn scaling_preserves_fractional_remainder_across_steps() {
    let (first, remainder) = checked_mul_div_with_remainder(1, 1, 3, 0)
        .unwrap_or_else(|| panic!("first exact scaling unexpectedly overflowed"));
    let (second, remainder) = checked_mul_div_with_remainder(1, 1, 3, remainder)
        .unwrap_or_else(|| panic!("second exact scaling unexpectedly overflowed"));
    let (third, remainder) = checked_mul_div_with_remainder(1, 1, 3, remainder)
        .unwrap_or_else(|| panic!("third exact scaling unexpectedly overflowed"));

    assert_eq!((first, second, third), (0, 0, 1));
    assert_eq!(remainder, 0);
}

#[test]
fn scaling_avoids_overflow_when_only_the_unreduced_product_is_too_wide() {
    let value = u128::MAX;
    let divisor = 1_000_000_000;
    let (whole, remainder) = checked_mul_div_with_remainder(value, divisor, divisor, 0)
        .unwrap_or_else(|| panic!("exactly cancelling scale unexpectedly overflowed"));

    assert_eq!(whole, value);
    assert_eq!(remainder, 0);
}

#[test]
fn scaling_reports_overflow_when_the_authoritative_result_is_too_wide() {
    assert_eq!(checked_mul_div_with_remainder(u128::MAX, 2, 1, 0), None);
}
