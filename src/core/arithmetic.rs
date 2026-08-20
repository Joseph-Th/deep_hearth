//! Exact integer scaling helpers shared by authoritative physical integrations.

/// Calculates `value * multiplier / divisor` plus the exact carried remainder without requiring
/// the full-width product to fit in `u128`.
///
/// `prior_remainder` uses the same denominator and must be smaller than `divisor`. Returning
/// `None` means a checked intermediate or the whole-number result exceeds `u128`. Decomposing both
/// factors around the divisor avoids overflow solely from the unreduced full-width product.
pub(crate) fn checked_mul_div_with_remainder(
    value: u128,
    multiplier: u128,
    divisor: u128,
    prior_remainder: u128,
) -> Option<(u128, u128)> {
    debug_assert!(divisor != 0);
    debug_assert!(prior_remainder < divisor);

    let value_whole = value / divisor;
    let value_remainder = value % divisor;
    let multiplier_whole = multiplier / divisor;
    let multiplier_remainder = multiplier % divisor;

    let whole_from_value = value_whole.checked_mul(multiplier)?;
    let whole_from_multiplier = value_remainder.checked_mul(multiplier_whole)?;
    let fractional_numerator = value_remainder
        .checked_mul(multiplier_remainder)?
        .checked_add(prior_remainder)?;
    let fractional_whole = fractional_numerator / divisor;
    let remainder = fractional_numerator % divisor;

    whole_from_value
        .checked_add(whole_from_multiplier)?
        .checked_add(fractional_whole)
        .map(|whole| (whole, remainder))
}

#[cfg(all(
    test,
    any(not(feature = "test-unit-sharded"), feature = "test-unit-foundation")
))]
mod tests {
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
}
