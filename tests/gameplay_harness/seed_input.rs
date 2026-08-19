//! Shared parsing for explicit gameplay replay seeds.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SeedListError {
    Empty,
    Invalid { index: usize },
}

pub(super) fn parse_seed(raw: &str) -> Option<u64> {
    let trimmed = raw.trim();
    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        u64::from_str_radix(hex, 16).ok()
    } else {
        trimmed.parse().ok()
    }
}

pub(super) fn parse_seed_list(raw: &str) -> Result<Vec<u64>, SeedListError> {
    if raw.trim().is_empty() {
        return Err(SeedListError::Empty);
    }
    raw.split(',')
        .enumerate()
        .map(|(index, token)| parse_seed(token).ok_or(SeedListError::Invalid { index }))
        .collect()
}
