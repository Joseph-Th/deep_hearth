//! Replayable gameplay-harness world/scenario and behavior seed configuration.

use super::seed::mix64;
use super::seed_input::{SeedListError, parse_seed, parse_seed_list};

const GATE_VARIATION_SCENARIO_COUNT: usize = 2;
const EXPLORATORY_VARIATION_SCENARIO_COUNT: usize = 4;
const SEED_STRIDE: u64 = 0xD1B5_4A32_D192_ED03;

pub(super) use super::focused_seeds::MAINTAINED_VARIATION_ROOT;
pub(super) const MAINTAINED_BEHAVIOR_ROOT: u64 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MaintainedAnchor {
    NormalBaseline,
    WarningMaintenance,
    CriticalMaintenance,
    AdaptiveEnergy,
    ManualRecovery,
    SurvivalRecovery,
}

impl MaintainedAnchor {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::NormalBaseline => "normal-baseline",
            Self::WarningMaintenance => "warning-maintenance",
            Self::CriticalMaintenance => "critical-maintenance",
            Self::AdaptiveEnergy => "adaptive-energy",
            Self::ManualRecovery => "manual-recovery",
            Self::SurvivalRecovery => "survival-recovery",
        }
    }

    const fn behavior_slot(self) -> usize {
        match self {
            Self::NormalBaseline => 0,
            Self::WarningMaintenance => 1,
            Self::CriticalMaintenance => 2,
            Self::AdaptiveEnergy => 3,
            Self::ManualRecovery => 4,
            Self::SurvivalRecovery => 5,
        }
    }
}

const MAINTAINED_ANCHORS: [(MaintainedAnchor, u64); 6] = [
    (MaintainedAnchor::NormalBaseline, 1),
    (MaintainedAnchor::WarningMaintenance, 4),
    (MaintainedAnchor::CriticalMaintenance, 9),
    (MaintainedAnchor::AdaptiveEnergy, 19),
    (MaintainedAnchor::ManualRecovery, 380),
    (MaintainedAnchor::SurvivalRecovery, 0x1F65_DBFE_4A87_A054),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ScenarioPlanMode {
    Gate,
    Explore,
}

fn parse_scenario_seed_list(raw: &str) -> Result<Vec<u64>, GameplayHarnessConfigError> {
    match parse_seed_list(raw) {
        Ok(seeds) => Ok(seeds),
        Err(SeedListError::Empty) => Err(GameplayHarnessConfigError::EmptyScenarioSeedList),
        Err(SeedListError::Invalid { index }) => {
            Err(GameplayHarnessConfigError::InvalidScenarioSeed { index })
        }
    }
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
    AnchorVariation,
    Custom,
}

impl ScenarioSeedSource {
    const fn label(self) -> &'static str {
        match self {
            Self::AnchorVariation => "anchor+variation",
            Self::Custom => "custom",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ScenarioSeedPair {
    pub(super) world_seed: u64,
    pub(super) behavior_seed: u64,
    pub(super) anchor: Option<MaintainedAnchor>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ScenarioSeedPlan {
    source: ScenarioSeedSource,
    cases: Vec<ScenarioSeedPair>,
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

    pub(super) fn anchor_seed_count(&self) -> usize {
        self.cases
            .iter()
            .filter(|case| case.anchor.is_some())
            .count()
    }

    pub(super) fn maintained_case(&self, anchor: MaintainedAnchor) -> Option<&ScenarioSeedPair> {
        self.cases.iter().find(|case| case.anchor == Some(anchor))
    }

    pub(super) fn variation_seed_count(&self) -> usize {
        match self.source {
            ScenarioSeedSource::Custom => 0,
            ScenarioSeedSource::AnchorVariation => {
                self.cases.len().saturating_sub(self.anchor_seed_count())
            }
        }
    }

    pub(super) fn custom_seed_count(&self) -> usize {
        match self.source {
            ScenarioSeedSource::AnchorVariation => 0,
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

fn append_variation_seeds(seeds: &mut Vec<u64>, root: u64, count: usize) {
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

fn custom_cases(world_seeds: Vec<u64>, behavior_root: u64) -> Vec<ScenarioSeedPair> {
    world_seeds
        .into_iter()
        .enumerate()
        .map(|(index, world_seed)| ScenarioSeedPair {
            world_seed,
            behavior_seed: behavior_seed(behavior_root, index),
            anchor: None,
        })
        .collect()
}

fn maintained_cases() -> Vec<ScenarioSeedPair> {
    MAINTAINED_ANCHORS
        .iter()
        .copied()
        .map(|(anchor, world_seed)| ScenarioSeedPair {
            world_seed,
            behavior_seed: behavior_seed(MAINTAINED_BEHAVIOR_ROOT, anchor.behavior_slot()),
            anchor: Some(anchor),
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
            cases: custom_cases(world_seeds, behavior_seed_root),
            variation_seed: None,
            behavior_seed_root,
        });
    }

    let mut world_seeds = MAINTAINED_ANCHORS
        .iter()
        .map(|(_, world_seed)| *world_seed)
        .collect::<Vec<_>>();
    let variation_seed = resolve_variation_seed(variation_raw, default_variation_seed)?;
    let variation_count = match mode {
        ScenarioPlanMode::Gate => GATE_VARIATION_SCENARIO_COUNT,
        ScenarioPlanMode::Explore => EXPLORATORY_VARIATION_SCENARIO_COUNT,
    };
    append_variation_seeds(&mut world_seeds, variation_seed, variation_count);
    let mut cases = maintained_cases();
    cases.extend(
        world_seeds[MAINTAINED_ANCHORS.len()..]
            .iter()
            .copied()
            .enumerate()
            .map(|(index, world_seed)| ScenarioSeedPair {
                world_seed,
                behavior_seed: behavior_seed(behavior_seed_root, index),
                anchor: None,
            }),
    );
    Ok(ScenarioSeedPlan {
        source: ScenarioSeedSource::AnchorVariation,
        cases,
        variation_seed: Some(variation_seed),
        behavior_seed_root,
    })
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
            MAINTAINED_VARIATION_ROOT,
            MAINTAINED_BEHAVIOR_ROOT,
        )
    }

    #[test]
    fn seed_configuration_rejects_invalid_inputs_with_exact_error_location() {
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
        assert_eq!(plan.variation_seed_count(), 0);
        assert_eq!(plan.custom_seed_count(), 3);
        assert_eq!(plan.variation_seed, None);
        assert_eq!(plan.behavior_seed_root, 0xBEEF);
        assert!(plan.cases().iter().all(|case| case.anchor.is_none()));
        assert!(
            plan.cases()
                .windows(2)
                .all(|pair| pair[0].behavior_seed != pair[1].behavior_seed)
        );
    }

    #[test]
    fn default_gate_keeps_maintained_anchors_and_adds_a_bounded_variation_sample() {
        let plan = plan(ScenarioPlanMode::Gate, None, None, None)
            .unwrap_or_else(|error| panic!("default gate seed plan failed: {error:?}"));

        assert_eq!(plan.source, ScenarioSeedSource::AnchorVariation);
        assert_eq!(
            plan.cases()
                .iter()
                .filter_map(|case| case.anchor.map(|anchor| (anchor, case.world_seed)))
                .collect::<Vec<_>>(),
            MAINTAINED_ANCHORS
        );
        assert_eq!(plan.anchor_seed_count(), MAINTAINED_ANCHORS.len());
        assert_eq!(plan.variation_seed_count(), GATE_VARIATION_SCENARIO_COUNT);
        assert_eq!(plan.variation_seed, Some(MAINTAINED_VARIATION_ROOT));
        assert_eq!(plan.behavior_seed_root, MAINTAINED_BEHAVIOR_ROOT);
    }

    #[test]
    fn gate_keeps_anchors_stable_while_generated_cases_follow_supplied_roots() {
        let first = scenario_seeds_from(ScenarioPlanMode::Gate, None, None, None, 0x1111, 0x2222)
            .unwrap_or_else(|error| panic!("first gate-default plan failed: {error:?}"));
        let second = scenario_seeds_from(ScenarioPlanMode::Gate, None, None, None, 0xAAAA, 0xBBBB)
            .unwrap_or_else(|error| panic!("second gate-default plan failed: {error:?}"));

        assert_eq!(
            first
                .cases()
                .iter()
                .filter(|case| case.anchor.is_some())
                .collect::<Vec<_>>(),
            second
                .cases()
                .iter()
                .filter(|case| case.anchor.is_some())
                .collect::<Vec<_>>()
        );
        assert_ne!(
            first
                .cases()
                .iter()
                .filter(|case| case.anchor.is_none())
                .collect::<Vec<_>>(),
            second
                .cases()
                .iter()
                .filter(|case| case.anchor.is_none())
                .collect::<Vec<_>>()
        );
        assert_eq!(first.variation_seed, Some(0x1111));
        assert_eq!(first.behavior_seed_root, 0x2222);
        assert_eq!(second.variation_seed, Some(0xAAAA));
        assert_eq!(second.behavior_seed_root, 0xBBBB);
    }

    #[test]
    fn explicit_world_and_behavior_roots_replay_the_same_cases() {
        let first = plan(
            ScenarioPlanMode::Explore,
            None,
            Some("0xBAD"),
            Some("0xCAFE"),
        )
        .unwrap_or_else(|error| panic!("first variation seed plan failed: {error:?}"));
        let second = plan(
            ScenarioPlanMode::Explore,
            None,
            Some("0xBAD"),
            Some("0xCAFE"),
        )
        .unwrap_or_else(|error| panic!("second variation seed plan failed: {error:?}"));

        assert_eq!(first, second);
        assert_eq!(first.source, ScenarioSeedSource::AnchorVariation);
        assert_eq!(first.anchor_seed_count(), MAINTAINED_ANCHORS.len());
        assert_eq!(
            first
                .cases()
                .iter()
                .filter_map(|case| case.anchor.map(|anchor| (anchor, case.world_seed)))
                .collect::<Vec<_>>(),
            MAINTAINED_ANCHORS
        );
        assert_eq!(
            first.variation_seed_count(),
            EXPLORATORY_VARIATION_SCENARIO_COUNT
        );
        assert_eq!(first.custom_seed_count(), 0);
        assert!(
            first
                .cases()
                .iter()
                .filter(|case| case.anchor.is_none())
                .all(|case| !MAINTAINED_ANCHORS
                    .iter()
                    .any(|(_, world_seed)| *world_seed == case.world_seed))
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
                .filter(|case| case.anchor.is_some())
                .map(|case| case.behavior_seed)
                .collect::<Vec<_>>(),
            second
                .cases()
                .iter()
                .filter(|case| case.anchor.is_some())
                .map(|case| case.behavior_seed)
                .collect::<Vec<_>>()
        );
        assert_ne!(
            first
                .cases()
                .iter()
                .filter(|case| case.anchor.is_none())
                .map(|case| case.behavior_seed)
                .collect::<Vec<_>>(),
            second
                .cases()
                .iter()
                .filter(|case| case.anchor.is_none())
                .map(|case| case.behavior_seed)
                .collect::<Vec<_>>()
        );
    }
}
