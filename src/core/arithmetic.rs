//! Exact integer scaling helpers shared by authoritative physical integrations.

/// Greatest common divisor for canonical `u32` ratio normalization.
pub(crate) const fn greatest_common_divisor_u32(mut left: u32, mut right: u32) -> u32 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

/// Calculates `value * multiplier / divisor` plus the exact carried remainder without requiring
/// the full-width product to fit in `u128`.
///
/// `prior_remainder` uses the same denominator and must be smaller than `divisor`. Returning
/// `None` means the divisor/remainder contract is invalid or the final whole-number result exceeds
/// `u128`. The full-width fallback keeps partial values in quotient/remainder form, so an
/// unreduced intermediate product cannot cause a false overflow.
pub(crate) fn checked_mul_div_with_remainder(
    value: u128,
    multiplier: u128,
    divisor: u128,
    prior_remainder: u128,
) -> Option<(u128, u128)> {
    if divisor == 0 || prior_remainder >= divisor {
        return None;
    }

    if let Some(product) = value.checked_mul(multiplier)
        && let Some(total) = product.checked_add(prior_remainder)
    {
        return Some((total / divisor, total % divisor));
    }

    // Fall back to binary multiplication while keeping every partial in quotient/remainder form.
    // This avoids ever materializing `value * multiplier`, while still detecting overflow of the
    // final whole-number result rather than overflow of an irrelevant unreduced intermediate.
    let mut result = (0_u128, prior_remainder);
    let mut factor = (value / divisor, value % divisor);
    let mut remaining_multiplier = multiplier;
    while remaining_multiplier != 0 {
        if remaining_multiplier & 1 != 0 {
            result = checked_add_divided(result, factor, divisor)?;
        }
        remaining_multiplier >>= 1;
        if remaining_multiplier != 0 {
            factor = checked_add_divided(factor, factor, divisor)?;
        }
    }
    Some(result)
}

/// Ceil-rounded `value * multiplier / divisor` with the same full-width overflow behavior as
/// [`checked_mul_div_with_remainder`].
pub(crate) fn checked_mul_div_ceil(value: u128, multiplier: u128, divisor: u128) -> Option<u128> {
    let (whole, remainder) = checked_mul_div_with_remainder(value, multiplier, divisor, 0)?;
    if remainder == 0 {
        Some(whole)
    } else {
        whole.checked_add(1)
    }
}

fn checked_add_divided(
    left: (u128, u128),
    right: (u128, u128),
    divisor: u128,
) -> Option<(u128, u128)> {
    debug_assert!(divisor != 0);
    debug_assert!(left.1 < divisor);
    debug_assert!(right.1 < divisor);

    let carry_threshold = divisor - left.1;
    let (remainder, carry) = if right.1 >= carry_threshold {
        (right.1 - carry_threshold, 1_u128)
    } else {
        (left.1 + right.1, 0_u128)
    };
    let whole = left.0.checked_add(right.0)?.checked_add(carry)?;
    Some((whole, remainder))
}

/// Scales a full-width integer by a bounded fraction without overflowing an intermediate product.
///
/// The numerator may not exceed the denominator, so the result cannot exceed `value`. Both ratio
/// terms are `u32`, which also bounds the remainder product used by the quotient/remainder split.
pub(crate) fn scale_u128_fraction_floor(value: u128, numerator: u32, denominator: u32) -> u128 {
    assert!(denominator != 0, "fraction denominator must be nonzero");
    assert!(
        numerator <= denominator,
        "fraction numerator cannot exceed denominator"
    );
    if numerator == 0 || value == 0 {
        return 0;
    }
    if numerator == denominator {
        return value;
    }

    let denominator = u128::from(denominator);
    let numerator = u128::from(numerator);
    (value / denominator) * numerator + (value % denominator) * numerator / denominator
}

/// Ceil-rounded companion to [`scale_u128_fraction_floor`].
pub(crate) fn scale_u128_fraction_ceil(value: u128, numerator: u32, denominator: u32) -> u128 {
    assert!(denominator != 0, "fraction denominator must be nonzero");
    assert!(
        numerator <= denominator,
        "fraction numerator cannot exceed denominator"
    );
    if numerator == 0 || value == 0 {
        return 0;
    }
    if numerator == denominator {
        return value;
    }

    let denominator = u128::from(denominator);
    let numerator = u128::from(numerator);
    let whole = (value / denominator) * numerator;
    let remainder_product = (value % denominator) * numerator;
    whole + remainder_product.div_ceil(denominator)
}

/// Calculates `numerator / denominator * scale`, flooring fractional scale units and saturating only
/// when the final scaled ratio itself is wider than `u128`.
///
/// This is intended for read-only normalized ratios such as utilization. Unlike direct
/// `numerator * scale`, it remains exact when the unreduced product exceeds `u128`.
pub(crate) fn scaled_ratio_floor_saturating(
    numerator: u128,
    denominator: u128,
    scale: u32,
) -> u128 {
    assert!(denominator != 0, "ratio denominator must be nonzero");
    assert!(scale != 0, "ratio scale must be nonzero");
    checked_mul_div_with_remainder(numerator, u128::from(scale), denominator, 0)
        .map_or(u128::MAX, |(whole, _remainder)| whole)
}

#[cfg(test)]
#[path = "arithmetic_tests.rs"]
mod tests;
