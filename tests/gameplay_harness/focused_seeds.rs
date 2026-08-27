//! Replayable seed selection for anchored plus bounded-variation gameplay probes.

use super::seed::{mix64, unique_mixed_seed};
use super::seed_input::{SeedListError, parse_seed, parse_seed_list};

pub(super) const MAINTAINED_VARIATION_ROOT: u64 = 0xE7A1_0A7E_5EED_2026;
pub(super) const FOCUSED_VARIATION_COUNT: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FocusedProbeSeedError {
    InvalidVariationSeed,
    SeedList(SeedListError),
}

/// Resolves one maintained anchor plus a tiny replayable generated variation sample.
///
/// `DEEP_HEARTH_GAMEPLAY_SEEDS` remains the exact override for deliberate replay/sweeps. Otherwise
/// the caller supplies a bounded variation root. Maintained focused runners use a stable root while
/// explicit replay inputs can provide one directly. A probe-specific salt keeps concerns independent.
/// `DEEP_HEARTH_GAMEPLAY_VARIATION_SEED` selects the exact replayable sample root.
pub(super) fn focused_probe_seeds_from(
    scenario_raw: Option<&str>,
    variation_raw: Option<&str>,
    maintained_seed: u64,
    probe_salt: u64,
    default_variation_root: u64,
) -> Result<Vec<u64>, FocusedProbeSeedError> {
    if let Some(raw) = scenario_raw {
        return parse_seed_list(raw).map_err(FocusedProbeSeedError::SeedList);
    }
    let root = match variation_raw {
        Some(raw) => parse_seed(raw).ok_or(FocusedProbeSeedError::InvalidVariationSeed)?,
        None => default_variation_root,
    };
    let mut seeds = Vec::with_capacity(1 + FOCUSED_VARIATION_COUNT);
    seeds.push(maintained_seed);
    let mut variation = root ^ probe_salt;
    for index in 0..FOCUSED_VARIATION_COUNT {
        variation = mix64(
            variation
                ^ u64::try_from(index + 1)
                    .unwrap_or_else(|_| unreachable!("focused variation index fits u64"))
                    .wrapping_mul(0xD1B5_4A32_D192_ED03),
        );
        variation = unique_mixed_seed(variation, &seeds);
        seeds.push(variation);
    }
    Ok(seeds)
}
