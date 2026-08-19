//! Replayable seed selection for anchored plus bounded-variation gameplay probes.

use super::seed::mix64;
use super::seed_input::{SeedListError, parse_seed, parse_seed_list};

pub(super) const MAINTAINED_VARIATION_ROOT: u64 = 0xE7A1_0A7E_5EED_2026;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FocusedProbeSeedError {
    InvalidVariationSeed,
    SeedList(SeedListError),
}

/// Resolves one maintained anchor plus one replayable generated variation.
///
/// `DEEP_HEARTH_GAMEPLAY_SEEDS` remains the exact override for deliberate replay/sweeps. Otherwise
/// the caller supplies the bounded variation root, normally fresh for gameplay runs, and a probe-specific
/// salt keeps concerns independent. `DEEP_HEARTH_GAMEPLAY_VARIATION_SEED` reproduces an observed sample.
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
    let mut variation = mix64(root ^ probe_salt);
    while variation == maintained_seed {
        variation = mix64(variation);
    }
    Ok(vec![maintained_seed, variation])
}
