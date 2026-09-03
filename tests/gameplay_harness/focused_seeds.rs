//! Replayable seed selection for anchored plus bounded-variation gameplay probes.

use super::seed::{mix64, unique_mixed_seed};
use super::seed_input::{SeedListError, parse_seed, parse_seed_list};

pub(super) const GATE_VARIATION_COUNT: usize = 1;
pub(super) const EXPLORATORY_VARIATION_COUNT: usize = 4;

pub(super) fn probe_uses_actor_behavior(name: &str) -> bool {
    matches!(name, "survival-provisioning" | "woodworking")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FocusedProbeSeedError {
    InvalidVariationSeed,
    InvalidBehaviorSeed,
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
    world_seed: u64,
    behavior_seed: Option<u64>,
    role: FocusedProbeRole,
}

pub(super) struct FocusedProbeSeedPlan<'a> {
    pub(super) variation_count: usize,
    pub(super) scenario_raw: Option<&'a str>,
    pub(super) variation_raw: Option<&'a str>,
    pub(super) behavior_raw: Option<&'a str>,
    pub(super) maintained_seed: u64,
    pub(super) maintained_coverage_seeds: &'a [u64],
    pub(super) probe_salt: u64,
    pub(super) default_variation_root: u64,
    pub(super) default_behavior_root: Option<u64>,
}

impl FocusedProbeCase {
    pub(super) const fn new(
        world_seed: u64,
        behavior_seed: Option<u64>,
        role: FocusedProbeRole,
    ) -> Self {
        Self {
            world_seed,
            behavior_seed,
            role,
        }
    }

    /// Physical/scenario variation only. Actor preferences must not feed back into this seed.
    pub(super) const fn seed(self) -> u64 {
        self.world_seed
    }

    pub(super) const fn behavior_seed(self) -> Option<u64> {
        self.behavior_seed
    }

    pub(super) const fn role(self) -> FocusedProbeRole {
        self.role
    }
}

/// Resolves maintained regression cases plus an optional bounded replayable variation sample.
///
/// `DEEP_HEARTH_GAMEPLAY_SEEDS` remains the exact override for deliberate replay/sweeps. Routine
/// focused gates keep maintained deterministic cases plus one replayable organic case; reports
/// sample a broader replayable organic set. A probe-specific salt keeps concerns independent. Physical and actor variation use independent
/// replay roots so changing a preference cannot silently change the world.
pub(super) fn focused_probe_cases_from(
    plan: FocusedProbeSeedPlan<'_>,
) -> Result<Vec<FocusedProbeCase>, FocusedProbeSeedError> {
    let FocusedProbeSeedPlan {
        variation_count,
        scenario_raw,
        variation_raw,
        behavior_raw,
        maintained_seed,
        maintained_coverage_seeds,
        probe_salt,
        default_variation_root,
        default_behavior_root,
    } = plan;
    let behavior_root = match (default_behavior_root, behavior_raw) {
        (Some(_), Some(raw)) => {
            Some(parse_seed(raw).ok_or(FocusedProbeSeedError::InvalidBehaviorSeed)?)
        }
        (Some(root), None) => Some(root),
        (None, None) => None,
        (None, Some(_)) => return Err(FocusedProbeSeedError::InvalidBehaviorSeed),
    };
    if let Some(raw) = scenario_raw {
        return parse_seed_list(raw)
            .map(|seeds| {
                seeds
                    .into_iter()
                    .enumerate()
                    .map(|(index, world_seed)| {
                        FocusedProbeCase::new(
                            world_seed,
                            behavior_root.map(|root| behavior_seed(root, probe_salt, index)),
                            FocusedProbeRole::ExplicitReplay,
                        )
                    })
                    .collect()
            })
            .map_err(FocusedProbeSeedError::SeedList);
    }
    let mut raw_seeds = Vec::with_capacity(1 + maintained_coverage_seeds.len() + variation_count);
    raw_seeds.push(maintained_seed);
    let mut cases = Vec::with_capacity(1 + maintained_coverage_seeds.len() + variation_count);
    cases.push(FocusedProbeCase::new(
        maintained_seed,
        behavior_root.map(|_| maintained_behavior_seed(maintained_seed, probe_salt)),
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
            behavior_root.map(|_| maintained_behavior_seed(coverage_seed, probe_salt)),
            FocusedProbeRole::MaintainedCoverage,
        ));
    }
    if variation_count == 0 {
        return Ok(cases);
    }
    let root = match variation_raw {
        Some(raw) => parse_seed(raw).ok_or(FocusedProbeSeedError::InvalidVariationSeed)?,
        None => default_variation_root,
    };
    let mut variation = root ^ probe_salt;
    for index in 0..variation_count {
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
            behavior_root.map(|root| behavior_seed(root, probe_salt, index)),
            FocusedProbeRole::OrganicVariation,
        ));
    }
    Ok(cases)
}

fn maintained_behavior_seed(world_seed: u64, probe_salt: u64) -> u64 {
    mix64(world_seed ^ probe_salt.rotate_left(19) ^ 0x4D41_494E_5441_494E)
}

fn behavior_seed(root: u64, probe_salt: u64, index: usize) -> u64 {
    let ordinal = u64::try_from(index + 1)
        .unwrap_or_else(|_| unreachable!("focused behavior variation index fits u64"));
    let mixed =
        mix64(root ^ probe_salt.rotate_left(31) ^ ordinal.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    // Keep almost all actor entropy fresh while deliberately stratifying one generic behavior bit.
    // Exploratory focused probes use a small organic set, so this prevents the bounded sample from
    // accidentally collapsing to one binary preference without coupling physical world generation
    // to actor behavior. The root controls which stratum appears first and remains fully replayable.
    (mixed & !1) | ((root ^ ordinal) & 1)
}
