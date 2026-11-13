//! Exact player-entered solid quantities, independent of the action that owns them.

use deep_hearth::core::quantity::Mass;

/// Parse all player-selected solid-matter quantities at inventory's exact
/// milligram precision. Gathering, extraction and hand processing must use
/// the same representation instead of stranding fractional physical remains.
pub(super) fn parse_decimal_grams(text: &str, quantity: &str) -> Result<Mass, String> {
    let invalid = || format!("{quantity} must be decimal grams (up to three fractional digits)");
    let (whole, fractional) = match text.split_once('.') {
        Some((whole, fractional)) if (1..=3).contains(&fractional.len()) => {
            (whole, Some(fractional))
        }
        Some(_) => return Err(invalid()),
        None => (text, None),
    };
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || fractional.is_some_and(|fraction| !fraction.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return Err(invalid());
    }
    let grams = whole
        .parse::<u64>()
        .map_err(|_| format!("{quantity} is too large"))?;
    let fractional_milligrams = match fractional {
        Some(fraction) => {
            let digits = fraction.parse::<u64>().map_err(|_| invalid())?;
            digits
                * match fraction.len() {
                    1 => 100,
                    2 => 10,
                    3 => 1,
                    _ => unreachable!("fractional digits already validated"),
                }
        }
        None => 0,
    };
    let milligrams = grams
        .checked_mul(1_000)
        .and_then(|amount| amount.checked_add(fractional_milligrams))
        .ok_or_else(|| format!("{quantity} is too large"))?;
    Ok(Mass::from_milligrams(milligrams))
}
