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

#[cfg(test)]
#[path = "arithmetic_tests.rs"]
mod tests;
