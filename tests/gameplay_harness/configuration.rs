//! Replayable gameplay-harness world/scenario and behavior seed configuration.

use super::seed::{mix64, unique_mixed_seed};
use super::seed_input::{SeedListError, parse_seed, parse_seed_list};

#[cfg(test)]
const GATE_VARIATION_SCENARIO_COUNT: usize = 1;
const EXPLORATORY_VARIATION_SCENARIO_COUNT: usize = 4;
const SEED_STRIDE: u64 = 0xD1B5_4A32_D192_ED03;

pub(super) use super::seed::MAINTAINED_VARIATION_ROOT;
pub(super) const MAINTAINED_BEHAVIOR_ROOT: u64 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MaintainedAnchor {
    NormalBaseline,
    WarningMaintenance,
    CriticalMaintenance,
    ConditionPressure,
    AdaptiveEnergy,
    ManualRecovery,
    SurvivalRecovery,
}

impl MaintainedAnchor {
    pub(super) const ALL: [Self; 7] = [
        Self::NormalBaseline,
        Self::WarningMaintenance,
        Self::CriticalMaintenance,
        Self::ConditionPressure,
        Self::AdaptiveEnergy,
        Self::ManualRecovery,
        Self::SurvivalRecovery,
    ];

    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::NormalBaseline => "normal-baseline",
            Self::WarningMaintenance => "warning-maintenance",
            Self::CriticalMaintenance => "critical-maintenance",
            Self::ConditionPressure => "condition-pressure",
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
            Self::ConditionPressure => 7,
        }
    }
}

const MAINTAINED_ANCHORS: [(MaintainedAnchor, u64); 7] = [
    (MaintainedAnchor::NormalBaseline, 1),
    (MaintainedAnchor::WarningMaintenance, 4),
    (MaintainedAnchor::CriticalMaintenance, 9),
    (MaintainedAnchor::ConditionPressure, 29),
    (MaintainedAnchor::AdaptiveEnergy, 19),
    (MaintainedAnchor::ManualRecovery, 380),
    (MaintainedAnchor::SurvivalRecovery, 0x1F65_DBFE_4A87_A054),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ScenarioPlanMode {
    #[cfg(test)]
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
        match (self.source, self.variation_seed) {
            (ScenarioSeedSource::AnchorVariation, None) => "maintained",
            (ScenarioSeedSource::AnchorVariation, Some(_)) => "anchor+variation",
            (ScenarioSeedSource::Custom, _) => "custom",
        }
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
        candidate = unique_mixed_seed(candidate, seeds);
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
    if let Some(raw) = scenario_raw {
        let behavior_seed_root = resolve_behavior_seed(behavior_raw, default_behavior_seed)?;
        let world_seeds = parse_scenario_seed_list(raw)?;
        return Ok(ScenarioSeedPlan {
            source: ScenarioSeedSource::Custom,
            cases: custom_cases(world_seeds, behavior_seed_root),
            variation_seed: None,
            behavior_seed_root,
        });
    }

    let variation_count = match mode {
        #[cfg(test)]
        ScenarioPlanMode::Gate => GATE_VARIATION_SCENARIO_COUNT,
        ScenarioPlanMode::Explore => EXPLORATORY_VARIATION_SCENARIO_COUNT,
    };
    let behavior_seed_root = resolve_behavior_seed(behavior_raw, default_behavior_seed)?;
    let variation_seed = Some(resolve_variation_seed(
        variation_raw,
        default_variation_seed,
    )?);
    let mut world_seeds = MAINTAINED_ANCHORS
        .iter()
        .map(|(_, world_seed)| *world_seed)
        .collect::<Vec<_>>();
    if let Some(variation_seed) = variation_seed {
        append_variation_seeds(&mut world_seeds, variation_seed, variation_count);
    }
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
        variation_seed,
        behavior_seed_root,
    })
}
