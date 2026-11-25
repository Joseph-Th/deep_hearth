//! Replayable gameplay-harness physical scenario seed configuration.

use super::seed::{mix64, unique_mixed_seed};
use super::seed_input::{SeedListError, parse_seed, parse_seed_list};

const EXPLORATORY_VARIATION_SCENARIO_COUNT: usize = 4;
const SEED_STRIDE: u64 = 0xD1B5_4A32_D192_ED03;

pub(super) use super::seed::MAINTAINED_VARIATION_ROOT;

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
    EmptyScenarioSeedList,
    InvalidScenarioSeed { index: usize },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScenarioSeedSource {
    Maintained,
    MaintainedVariation,
    Custom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ScenarioSeedCase {
    pub(super) world_seed: u64,
    pub(super) anchor: Option<MaintainedAnchor>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ScenarioSeedPlan {
    source: ScenarioSeedSource,
    cases: Vec<ScenarioSeedCase>,
    variation_seed: Option<u64>,
}

impl ScenarioSeedPlan {
    pub(super) const fn source_label(&self) -> &'static str {
        match self.source {
            ScenarioSeedSource::Maintained => "maintained",
            ScenarioSeedSource::MaintainedVariation => "maintained+variation",
            ScenarioSeedSource::Custom => "custom",
        }
    }

    pub(super) fn cases(&self) -> &[ScenarioSeedCase] {
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
            ScenarioSeedSource::Maintained | ScenarioSeedSource::Custom => 0,
            ScenarioSeedSource::MaintainedVariation => {
                self.cases.len().saturating_sub(self.anchor_seed_count())
            }
        }
    }

    pub(super) fn custom_seed_count(&self) -> usize {
        match self.source {
            ScenarioSeedSource::Maintained | ScenarioSeedSource::MaintainedVariation => 0,
            ScenarioSeedSource::Custom => self.cases.len(),
        }
    }

    pub(super) fn variation_label(&self) -> String {
        self.variation_seed
            .map(|seed| format!("0x{seed:016X}"))
            .unwrap_or_else(|| "n/a".to_owned())
    }

    pub(super) fn replay_label(&self) -> String {
        if self.source == ScenarioSeedSource::Maintained {
            return "maintained".to_owned();
        }
        self.cases
            .iter()
            .map(|case| format!("0x{:016X}", case.world_seed))
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

fn append_variation_seeds(seeds: &mut Vec<u64>, root: u64, count: usize) {
    let mut candidate = root;
    for index in 0..count {
        candidate = mix64(candidate ^ (index as u64 + 1).wrapping_mul(SEED_STRIDE));
        candidate = unique_mixed_seed(candidate, seeds);
        seeds.push(candidate);
    }
}

fn custom_cases(world_seeds: Vec<u64>) -> Vec<ScenarioSeedCase> {
    world_seeds
        .into_iter()
        .map(|world_seed| ScenarioSeedCase {
            world_seed,
            anchor: None,
        })
        .collect()
}

fn maintained_cases() -> Vec<ScenarioSeedCase> {
    MAINTAINED_ANCHORS
        .iter()
        .copied()
        .map(|(anchor, world_seed)| ScenarioSeedCase {
            world_seed,
            anchor: Some(anchor),
        })
        .collect()
}

pub(super) fn scenario_seeds_from(
    mode: ScenarioPlanMode,
    scenario_raw: Option<&str>,
    variation_raw: Option<&str>,
    default_variation_seed: u64,
) -> Result<ScenarioSeedPlan, GameplayHarnessConfigError> {
    if let Some(raw) = scenario_raw {
        let world_seeds = parse_scenario_seed_list(raw)?;
        return Ok(ScenarioSeedPlan {
            source: ScenarioSeedSource::Custom,
            cases: custom_cases(world_seeds),
            variation_seed: None,
        });
    }

    let variation_count = match mode {
        #[cfg(test)]
        // The runner supplies at most one organic root to a routine workshop gate. A broad audit
        // may omit it when another gameplay scope owns that run's organic sampling budget.
        ScenarioPlanMode::Gate => usize::from(variation_raw.is_some()),
        ScenarioPlanMode::Explore => EXPLORATORY_VARIATION_SCENARIO_COUNT,
    };
    let variation_seed = (variation_count > 0)
        .then(|| resolve_variation_seed(variation_raw, default_variation_seed))
        .transpose()?;
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
            .map(|world_seed| ScenarioSeedCase {
                world_seed,
                anchor: None,
            }),
    );
    Ok(ScenarioSeedPlan {
        source: if variation_count == 0 {
            ScenarioSeedSource::Maintained
        } else {
            ScenarioSeedSource::MaintainedVariation
        },
        cases,
        variation_seed,
    })
}
