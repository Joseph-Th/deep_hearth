//! Tests for the sibling arithmetic module; isolated so test-only edits do not invalidate production builds.

use super::*;

#[test]
fn greatest_common_divisor_handles_zero_and_coprime_inputs() {
    assert_eq!(greatest_common_divisor_u32(0, 0), 0);
    assert_eq!(greatest_common_divisor_u32(0, 42), 42);
    assert_eq!(greatest_common_divisor_u32(84, 30), 6);
    assert_eq!(greatest_common_divisor_u32(u32::MAX, u32::MAX - 1), 1);
}

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
fn ceil_scaling_handles_full_width_fractional_products() {
    let divisor = u128::MAX - 17;
    let value = divisor - 1;
    assert_eq!(
        checked_mul_div_ceil(value, 1_000_000, divisor),
        Some(1_000_000)
    );
    assert_eq!(checked_mul_div_ceil(u128::MAX, 2, 1), None);
}

#[test]
fn scaling_handles_near_full_width_divisors_without_overflowing_the_fractional_product() {
    let divisor = u128::MAX - 17;
    let value = divisor - 1;
    let multiplier = 1_000_000_u128;
    let (whole, remainder) = checked_mul_div_with_remainder(value, multiplier, divisor, 0)
        .unwrap_or_else(|| panic!("full-width fractional scaling unexpectedly overflowed"));

    assert_eq!(whole, multiplier - 1);
    assert_eq!(remainder, divisor - multiplier);

    let (whole, remainder) = checked_mul_div_with_remainder(value, multiplier, divisor, multiplier)
        .unwrap_or_else(|| panic!("full-width carried scaling unexpectedly overflowed"));
    assert_eq!(whole, multiplier);
    assert_eq!(remainder, 0);
}

#[test]
fn scaling_rejects_invalid_divisor_or_remainder() {
    assert_eq!(checked_mul_div_with_remainder(1, 1, 0, 0), None);
    assert_eq!(checked_mul_div_with_remainder(1, 1, 3, 3), None);
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

#[test]
fn bounded_fraction_scaling_handles_full_width_values_without_intermediate_overflow() {
    let value = u128::MAX;
    assert_eq!(
        scale_u128_fraction_floor(value, 500_000, 1_000_000),
        value / 2
    );
    assert_eq!(
        scale_u128_fraction_ceil(value, 500_000, 1_000_000),
        value.div_ceil(2)
    );
    assert_eq!(
        scale_u128_fraction_floor(value, 1_000_000, 1_000_000),
        value
    );
}

#[test]
fn scaled_ratio_is_exact_when_the_naive_product_would_overflow() {
    let denominator = u128::MAX - 17;
    let numerator = denominator - 1;
    assert_eq!(
        scaled_ratio_floor_saturating(numerator, denominator, 1_000_000),
        999_999
    );
    assert_eq!(
        scaled_ratio_floor_saturating(denominator, denominator, 1_000_000),
        1_000_000
    );
    assert_eq!(
        scaled_ratio_floor_saturating(u128::MAX, 1, 1_000_000),
        u128::MAX
    );
}
