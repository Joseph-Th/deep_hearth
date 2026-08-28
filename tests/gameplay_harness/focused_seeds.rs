//! Replayable seed selection for anchored plus bounded-variation gameplay probes.

use super::seed::{mix64, unique_mixed_seed};
use super::seed_input::{SeedListError, parse_seed, parse_seed_list};

pub(super) const MAINTAINED_VARIATION_ROOT: u64 = 0xE7A1_0A7E_5EED_2026;
pub(super) const FOCUSED_VARIATION_COUNT: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FocusedProbeSeedError {
    InvalidVariationSeed,
    SeedList(SeedListError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FocusedProbeRole {
    MaintainedAnchor,
    MaintainedCoverage,
    OrganicVariation,
    ExplicitReplay,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct FocusedProbeCase {
    seed: u64,
    role: FocusedProbeRole,
}

impl FocusedProbeCase {
    pub(super) const fn new(seed: u64, role: FocusedProbeRole) -> Self {
        Self { seed, role }
    }

    pub(super) const fn seed(self) -> u64 {
        self.seed
    }

    pub(super) const fn role(self) -> FocusedProbeRole {
        self.role
    }
}

/// Resolves a maintained anchor, stable alternate-path coverage, and a small fresh variation sample.
///
/// `DEEP_HEARTH_GAMEPLAY_SEEDS` remains the exact override for deliberate replay/sweeps. Otherwise
/// the caller supplies a bounded variation root. Normal focused runners supply a fresh replayable
/// root while explicit replay inputs can provide one directly. A probe-specific salt keeps concerns
/// independent.
/// `DEEP_HEARTH_GAMEPLAY_VARIATION_SEED` selects the exact replayable sample root.
pub(super) fn focused_probe_cases_from(
    scenario_raw: Option<&str>,
    variation_raw: Option<&str>,
    maintained_seed: u64,
    maintained_coverage_seeds: &[u64],
    probe_salt: u64,
    default_variation_root: u64,
) -> Result<Vec<FocusedProbeCase>, FocusedProbeSeedError> {
    if let Some(raw) = scenario_raw {
        return parse_seed_list(raw)
            .map(|seeds| {
                seeds
                    .into_iter()
                    .map(|seed| FocusedProbeCase::new(seed, FocusedProbeRole::ExplicitReplay))
                    .collect()
            })
            .map_err(FocusedProbeSeedError::SeedList);
    }
    let root = match variation_raw {
        Some(raw) => parse_seed(raw).ok_or(FocusedProbeSeedError::InvalidVariationSeed)?,
        None => default_variation_root,
    };
    let mut raw_seeds =
        Vec::with_capacity(1 + maintained_coverage_seeds.len() + FOCUSED_VARIATION_COUNT);
    raw_seeds.push(maintained_seed);
    let mut cases =
        Vec::with_capacity(1 + maintained_coverage_seeds.len() + FOCUSED_VARIATION_COUNT);
    cases.push(FocusedProbeCase::new(
        maintained_seed,
        FocusedProbeRole::MaintainedAnchor,
    ));
    for &coverage_seed in maintained_coverage_seeds {
        assert!(
            !raw_seeds.contains(&coverage_seed),
            "focused maintained coverage seeds must be distinct from the anchor and each other"
        );
        raw_seeds.push(coverage_seed);
        cases.push(FocusedProbeCase::new(
            coverage_seed,
            FocusedProbeRole::MaintainedCoverage,
        ));
    }
    let mut variation = root ^ probe_salt;
    for index in 0..FOCUSED_VARIATION_COUNT {
        variation = mix64(
            variation
                ^ u64::try_from(index + 1)
                    .unwrap_or_else(|_| unreachable!("focused variation index fits u64"))
                    .wrapping_mul(0xD1B5_4A32_D192_ED03),
        );
        variation = unique_mixed_seed(variation, &raw_seeds);
        raw_seeds.push(variation);
        cases.push(FocusedProbeCase::new(
            variation,
            FocusedProbeRole::OrganicVariation,
        ));
    }
    Ok(cases)
}
