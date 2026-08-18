//! Replayable gameplay-harness world/scenario and behavior seed configuration.

use super::seed::mix64;

const ANCHOR_WORLD_SEEDS: [u64; 5] = [1, 4, 9, 19, 380];
const GATE_ORGANIC_SCENARIO_COUNT: usize = 2;
const EXPLORATORY_ORGANIC_SCENARIO_COUNT: usize = 4;
const SEED_STRIDE: u64 = 0xD1B5_4A32_D192_ED03;

pub(super) const MAINTAINED_EXPLORATORY_WORLD_ROOT: u64 = 0xE7A1_0A7E_5EED_2026;
pub(super) const MAINTAINED_BEHAVIOR_ROOT: u64 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ScenarioPlanMode {
    Gate,
    Explore,
}

fn parse_scenario_seed_list(raw: &str) -> Result<Vec<u64>, GameplayHarnessConfigError> {
    if raw.trim().is_empty() {
        return Err(GameplayHarnessConfigError::EmptyScenarioSeedList);
    }
    raw.split(',')
        .enumerate()
        .map(|(index, token)| {
            parse_seed(token).ok_or(GameplayHarnessConfigError::InvalidScenarioSeed { index })
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GameplayHarnessConfigError {
    InvalidVariationSeed,
    InvalidBehaviorSeed,
    EmptyScenarioSeedList,
    InvalidScenarioSeed { index: usize },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScenarioSeedSource {
    AnchorOrganic,
    Custom,
}

impl ScenarioSeedSource {
    const fn label(self) -> &'static str {
        match self {
            Self::AnchorOrganic => "anchor+organic",
            Self::Custom => "custom",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ScenarioSeedPair {
    pub(super) world_seed: u64,
    pub(super) behavior_seed: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ScenarioSeedPlan {
    source: ScenarioSeedSource,
    cases: Vec<ScenarioSeedPair>,
    anchor_seed_count: usize,
    variation_seed: Option<u64>,
    behavior_seed_root: u64,
}

impl ScenarioSeedPlan {
    pub(super) const fn source_label(&self) -> &'static str {
        self.source.label()
    }

    pub(super) fn cases(&self) -> &[ScenarioSeedPair] {
        &self.cases
    }

    pub(super) const fn anchor_seed_count(&self) -> usize {
        self.anchor_seed_count
    }

    pub(super) fn organic_seed_count(&self) -> usize {
        match self.source {
            ScenarioSeedSource::Custom => 0,
            ScenarioSeedSource::AnchorOrganic => {
                self.cases.len().saturating_sub(self.anchor_seed_count)
            }
        }
    }

    pub(super) fn custom_seed_count(&self) -> usize {
        match self.source {
            ScenarioSeedSource::AnchorOrganic => 0,
            ScenarioSeedSource::Custom => self.cases.len(),
        }
    }

    pub(super) fn variation_label(&self) -> String {
        self.variation_seed
            .map(|seed| format!("0x{seed:016X}"))
            .unwrap_or_else(|| "n/a".to_owned())
    }

    pub(super) fn behavior_label(&self) -> String {
        format!("0x{:016X}", self.behavior_seed_root)
    }

    pub(super) fn replay_label(&self) -> String {
        self.cases
            .iter()
            .map(|case| format!("0x{:016X}@0x{:016X}", case.world_seed, case.behavior_seed))
            .collect::<Vec<_>>()
            .join(",")
    }
}

fn parse_seed(raw: &str) -> Option<u64> {
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

fn resolve_variation_seed(
    raw: Option<&str>,
    default_seed: u64,
) -> Result<u64, GameplayHarnessConfigError> {
    match raw {
        Some(text) => parse_seed(text).ok_or(GameplayHarnessConfigError::InvalidVariationSeed),
        None => Ok(default_seed),
    }
}

fn resolve_behavior_seed(
    raw: Option<&str>,
    default_seed: u64,
) -> Result<u64, GameplayHarnessConfigError> {
    match raw {
        Some(text) => parse_seed(text).ok_or(GameplayHarnessConfigError::InvalidBehaviorSeed),
        None => Ok(default_seed),
    }
}

fn append_organic_seeds(seeds: &mut Vec<u64>, root: u64, count: usize) {
    let mut candidate = root;
    for index in 0..count {
        candidate = mix64(candidate ^ (index as u64 + 1).wrapping_mul(SEED_STRIDE));
        while seeds.contains(&candidate) {
            candidate = mix64(candidate);
        }
        seeds.push(candidate);
    }
}

fn behavior_seed(root: u64, index: usize) -> u64 {
    mix64(root ^ (index as u64 + 1).wrapping_mul(SEED_STRIDE))
}

fn pair_worlds_with_behavior(
    world_seeds: Vec<u64>,
    anchor_count: usize,
    behavior_root: u64,
) -> Vec<ScenarioSeedPair> {
    world_seeds
        .into_iter()
        .enumerate()
        .map(|(index, world_seed)| ScenarioSeedPair {
            world_seed,
            behavior_seed: if index < anchor_count {
                behavior_seed(MAINTAINED_BEHAVIOR_ROOT, index)
            } else {
                behavior_seed(behavior_root, index - anchor_count)
            },
        })
        .collect()
}

pub(super) fn scenario_seeds_from(
    mode: ScenarioPlanMode,
    scenario_raw: Option<&str>,
    variation_raw: Option<&str>,
    behavior_raw: Option<&str>,
    default_variation_seed: u64,
    default_behavior_seed: u64,
) -> Result<ScenarioSeedPlan, GameplayHarnessConfigError> {
    let behavior_seed_root = resolve_behavior_seed(behavior_raw, default_behavior_seed)?;

    if let Some(raw) = scenario_raw {
        let world_seeds = parse_scenario_seed_list(raw)?;
        return Ok(ScenarioSeedPlan {
            source: ScenarioSeedSource::Custom,
            cases: pair_worlds_with_behavior(world_seeds, 0, behavior_seed_root),
            anchor_seed_count: 0,
            variation_seed: None,
            behavior_seed_root,
        });
    }

    let mut world_seeds = ANCHOR_WORLD_SEEDS.to_vec();
    let variation_seed = resolve_variation_seed(variation_raw, default_variation_seed)?;
    let organic_count = match mode {
        ScenarioPlanMode::Gate => GATE_ORGANIC_SCENARIO_COUNT,
        ScenarioPlanMode::Explore => EXPLORATORY_ORGANIC_SCENARIO_COUNT,
    };
    append_organic_seeds(&mut world_seeds, variation_seed, organic_count);
    Ok(ScenarioSeedPlan {
        source: ScenarioSeedSource::AnchorOrganic,
        cases: pair_worlds_with_behavior(world_seeds, ANCHOR_WORLD_SEEDS.len(), behavior_seed_root),
        anchor_seed_count: ANCHOR_WORLD_SEEDS.len(),
        variation_seed: Some(variation_seed),
        behavior_seed_root,
    })
}

/// Resolves a focused probe into one maintained anchor plus one fresh organic sample by default.
///
/// `DEEP_HEARTH_GAMEPLAY_SEEDS` remains the exact override for deliberate replay/sweeps. Otherwise
/// the same physical variation root used by the scenario matrix deterministically derives the
/// organic probe sample, with a probe-specific salt preventing different probes from collapsing to
/// the same case.
pub(super) fn focused_probe_seeds_from(
    scenario_raw: Option<&str>,
    variation_raw: Option<&str>,
    default_variation_seed: u64,
    maintained_seed: u64,
    probe_salt: u64,
) -> Result<Vec<u64>, GameplayHarnessConfigError> {
    if let Some(raw) = scenario_raw {
        return parse_scenario_seed_list(raw);
    }
    let root = resolve_variation_seed(variation_raw, default_variation_seed)?;
    let mut organic = mix64(root ^ probe_salt);
    while organic == maintained_seed {
        organic = mix64(organic);
    }
    Ok(vec![maintained_seed, organic])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(
        mode: ScenarioPlanMode,
        scenario_raw: Option<&str>,
        variation_raw: Option<&str>,
        behavior_raw: Option<&str>,
    ) -> Result<ScenarioSeedPlan, GameplayHarnessConfigError> {
        scenario_seeds_from(
            mode,
            scenario_raw,
            variation_raw,
            behavior_raw,
            MAINTAINED_EXPLORATORY_WORLD_ROOT,
            MAINTAINED_BEHAVIOR_ROOT,
        )
    }

    #[test]
    fn seed_parser_accepts_decimal_hex_and_u64_boundaries() {
        assert_eq!(parse_seed("  42  "), Some(42));
        assert_eq!(parse_seed("0x2A"), Some(42));
        assert_eq!(parse_seed("18446744073709551615"), Some(u64::MAX));
        assert_eq!(parse_seed("0xFFFFFFFFFFFFFFFF"), Some(u64::MAX));
    }

    #[test]
    fn seed_configuration_rejects_invalid_inputs_with_exact_error_location() {
        assert_eq!(parse_seed(""), None);
        assert_eq!(parse_seed("not-a-seed"), None);
        assert_eq!(
            resolve_variation_seed(Some("nope"), 1),
            Err(GameplayHarnessConfigError::InvalidVariationSeed)
        );
        assert_eq!(
            resolve_behavior_seed(Some("nope"), 1),
            Err(GameplayHarnessConfigError::InvalidBehaviorSeed)
        );
        assert_eq!(
            plan(ScenarioPlanMode::Gate, Some("1,nope,4"), None, None),
            Err(GameplayHarnessConfigError::InvalidScenarioSeed { index: 1 })
        );
        assert_eq!(
            plan(ScenarioPlanMode::Gate, Some(""), None, None),
            Err(GameplayHarnessConfigError::EmptyScenarioSeedList)
        );
    }

    #[test]
    fn custom_world_seed_list_is_exact_and_behavior_is_a_separate_channel() {
        let plan = plan(
            ScenarioPlanMode::Gate,
            Some("1, 0x2A,3"),
            Some("ignored"),
            Some("0xBEEF"),
        )
        .unwrap_or_else(|error| panic!("custom seed plan failed: {error:?}"));

        assert_eq!(
            plan.cases()
                .iter()
                .map(|case| case.world_seed)
                .collect::<Vec<_>>(),
            [1, 42, 3]
        );
        assert_eq!(plan.source, ScenarioSeedSource::Custom);
        assert_eq!(plan.anchor_seed_count(), 0);
        assert_eq!(plan.organic_seed_count(), 0);
        assert_eq!(plan.custom_seed_count(), 3);
        assert_eq!(plan.variation_seed, None);
        assert_eq!(plan.behavior_seed_root, 0xBEEF);
        assert!(
            plan.cases()
                .windows(2)
                .all(|pair| pair[0].behavior_seed != pair[1].behavior_seed)
        );
    }

    #[test]
    fn default_gate_keeps_maintained_anchors_and_adds_a_bounded_organic_sample() {
        let plan = plan(ScenarioPlanMode::Gate, None, None, None)
            .unwrap_or_else(|error| panic!("default gate seed plan failed: {error:?}"));

        assert_eq!(plan.source, ScenarioSeedSource::AnchorOrganic);
        assert_eq!(
            plan.cases()
                .iter()
                .take(ANCHOR_WORLD_SEEDS.len())
                .map(|case| case.world_seed)
                .collect::<Vec<_>>(),
            ANCHOR_WORLD_SEEDS
        );
        assert_eq!(plan.anchor_seed_count(), ANCHOR_WORLD_SEEDS.len());
        assert_eq!(plan.organic_seed_count(), GATE_ORGANIC_SCENARIO_COUNT);
        assert_eq!(plan.variation_seed, Some(MAINTAINED_EXPLORATORY_WORLD_ROOT));
        assert_eq!(plan.behavior_seed_root, MAINTAINED_BEHAVIOR_ROOT);
    }

    #[test]
    fn explicit_world_and_behavior_roots_replay_the_same_cases() {
        let first = plan(
            ScenarioPlanMode::Explore,
            None,
            Some("0xBAD"),
            Some("0xCAFE"),
        )
        .unwrap_or_else(|error| panic!("first organic seed plan failed: {error:?}"));
        let second = plan(
            ScenarioPlanMode::Explore,
            None,
            Some("0xBAD"),
            Some("0xCAFE"),
        )
        .unwrap_or_else(|error| panic!("second organic seed plan failed: {error:?}"));

        assert_eq!(first, second);
        assert_eq!(first.source, ScenarioSeedSource::AnchorOrganic);
        assert_eq!(first.anchor_seed_count(), ANCHOR_WORLD_SEEDS.len());
        assert_eq!(
            first
                .cases()
                .iter()
                .take(ANCHOR_WORLD_SEEDS.len())
                .map(|case| case.world_seed)
                .collect::<Vec<_>>(),
            ANCHOR_WORLD_SEEDS
        );
        assert_eq!(
            first.organic_seed_count(),
            EXPLORATORY_ORGANIC_SCENARIO_COUNT
        );
        assert_eq!(first.custom_seed_count(), 0);
        assert!(
            first.cases()[ANCHOR_WORLD_SEEDS.len()..]
                .iter()
                .all(|case| !ANCHOR_WORLD_SEEDS.contains(&case.world_seed))
        );
        assert_eq!(first.variation_seed, Some(0xBAD));
        assert_eq!(first.behavior_seed_root, 0xCAFE);
    }

    #[test]
    fn changing_behavior_root_does_not_change_world_cases() {
        let first = plan(ScenarioPlanMode::Explore, None, Some("0xBAD"), Some("1"))
            .unwrap_or_else(|error| panic!("first behavior plan failed: {error:?}"));
        let second = plan(ScenarioPlanMode::Explore, None, Some("0xBAD"), Some("2"))
            .unwrap_or_else(|error| panic!("second behavior plan failed: {error:?}"));

        assert_eq!(
            first
                .cases()
                .iter()
                .map(|case| case.world_seed)
                .collect::<Vec<_>>(),
            second
                .cases()
                .iter()
                .map(|case| case.world_seed)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            first
                .cases()
                .iter()
                .take(ANCHOR_WORLD_SEEDS.len())
                .map(|case| case.behavior_seed)
                .collect::<Vec<_>>(),
            second
                .cases()
                .iter()
                .take(ANCHOR_WORLD_SEEDS.len())
                .map(|case| case.behavior_seed)
                .collect::<Vec<_>>()
        );
        assert_ne!(
            first
                .cases()
                .iter()
                .skip(ANCHOR_WORLD_SEEDS.len())
                .map(|case| case.behavior_seed)
                .collect::<Vec<_>>(),
            second
                .cases()
                .iter()
                .skip(ANCHOR_WORLD_SEEDS.len())
                .map(|case| case.behavior_seed)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn focused_probe_default_keeps_one_anchor_and_adds_one_replayable_organic_case() {
        let first = focused_probe_seeds_from(None, Some("0xBAD"), 1, 0x1111, 0x2222)
            .unwrap_or_else(|error| panic!("first focused probe plan failed: {error:?}"));
        let second = focused_probe_seeds_from(None, Some("0xBAD"), 9, 0x1111, 0x2222)
            .unwrap_or_else(|error| panic!("second focused probe plan failed: {error:?}"));

        assert_eq!(first, second);
        assert_eq!(first.len(), 2);
        assert_eq!(first[0], 0x1111);
        assert_ne!(first[1], first[0]);
    }

    #[test]
    fn focused_probe_custom_seed_list_is_exact() {
        let seeds = focused_probe_seeds_from(Some("1,0x2A,3"), Some("ignored"), 9, 0x1111, 0x2222)
            .unwrap_or_else(|error| panic!("custom focused probe plan failed: {error:?}"));

        assert_eq!(seeds, [1, 42, 3]);
    }
}
