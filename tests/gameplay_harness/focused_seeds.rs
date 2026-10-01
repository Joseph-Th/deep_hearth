//! Replayable seed selection for anchored plus bounded-variation gameplay probes.

use super::focused_case::{FocusedProbeCase, FocusedProbeRole};
use super::seed::{mix64, unique_mixed_seed};
use super::seed_input::{SeedListError, parse_seed, parse_seed_list};

pub(super) const EXPLORATORY_VARIATION_COUNT: usize = 4;

/// Returns the bounded organic sample size used by an exploratory report.
///
/// Fieldwork combines independent demand, reserve scale, geology, and material-access regimes, so
/// four worlds are too sparse to tell whether its investment choices emerge outside maintained
/// witnesses. Doubling only that exploratory sample keeps routine gates unchanged and cheap.
pub(super) fn exploratory_variation_count(name: &str) -> usize {
    if name == "fieldwork" {
        EXPLORATORY_VARIATION_COUNT * 2
    } else {
        EXPLORATORY_VARIATION_COUNT
    }
}

pub(super) fn probe_uses_behavior_seed(name: &str) -> bool {
    matches!(
        name,
        "survival-provisioning" | "woodworking" | "fieldwork" | "power-provider" | "settlement"
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FocusedProbeSeedError {
    InvalidVariationSeed,
    InvalidBehaviorSeed,
    SeedList(SeedListError),
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

/// Resolves maintained contract cases plus an optional bounded replayable variation sample.
///
/// `DEEP_HEARTH_GAMEPLAY_SEEDS` remains the exact override for deliberate replay/sweeps. Routine
/// focused gates run maintained witnesses plus one runner-seeded organic case; explicit replay roots
/// replace that fresh case, while exploratory reports generate and run a broader sample.
/// A probe-specific salt keeps concerns independent. Physical and actor variation use independent
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
    // Keep almost all actor entropy fresh while deliberately stratifying the two low policy bits.
    // Four-case exploratory reports therefore cover all combinations used by simple binary actor
    // preferences (for example diet and work-hydration policy) without coupling physical world
    // generation to actor behavior. The root rotates the stratum order and remains replayable.
    let stratum = root.wrapping_add(ordinal - 1) & 0b11;
    (mixed & !0b11) | stratum
}
