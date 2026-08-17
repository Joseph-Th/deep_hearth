//! Replayable gameplay-harness seed parsing and deterministic scenario-plan configuration.

use std::time::{SystemTime, UNIX_EPOCH};

use super::seed::mix64;

const ANCHOR_SEEDS: [u64; 5] = [1, 4, 9, 19, 380];
const ORGANIC_SCENARIO_COUNT: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GameplayHarnessConfigError {
    InvalidVariationSeed,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ScenarioSeedPlan {
    source: ScenarioSeedSource,
    seeds: Vec<u64>,
    anchor_seed_count: usize,
    variation_seed: Option<u64>,
}

impl ScenarioSeedPlan {
    pub(super) const fn source_label(&self) -> &'static str {
        self.source.label()
    }

    pub(super) fn seeds(&self) -> &[u64] {
        &self.seeds
    }

    pub(super) const fn anchor_seed_count(&self) -> usize {
        self.anchor_seed_count
    }

    pub(super) fn organic_seed_count(&self) -> usize {
        match self.source {
            ScenarioSeedSource::AnchorOrganic => {
                self.seeds.len().saturating_sub(self.anchor_seed_count)
            }
            ScenarioSeedSource::Custom => 0,
        }
    }

    pub(super) fn custom_seed_count(&self) -> usize {
        match self.source {
            ScenarioSeedSource::AnchorOrganic => 0,
            ScenarioSeedSource::Custom => self.seeds.len(),
        }
    }

    pub(super) fn variation_label(&self) -> String {
        self.variation_seed
            .map(|seed| format!("0x{seed:016X}"))
            .unwrap_or_else(|| "n/a".to_owned())
    }

    pub(super) fn replay_label(&self) -> String {
        self.seeds
            .iter()
            .map(|seed| format!("0x{seed:016X}"))
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

fn generated_variation_seed() -> u64 {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let nanos = elapsed.as_nanos();
    let folded_time = nanos as u64 ^ (nanos >> 64) as u64;
    mix64(folded_time ^ u64::from(std::process::id()))
}

fn resolve_variation_seed(raw: Option<&str>) -> Result<u64, GameplayHarnessConfigError> {
    match raw {
        Some(text) => parse_seed(text).ok_or(GameplayHarnessConfigError::InvalidVariationSeed),
        None => Ok(generated_variation_seed()),
    }
}

fn append_organic_seeds(seeds: &mut Vec<u64>, root: u64) {
    let mut candidate = root;
    for index in 0..ORGANIC_SCENARIO_COUNT {
        candidate = mix64(candidate ^ (index as u64 + 1).wrapping_mul(0xD1B5_4A32_D192_ED03));
        while seeds.contains(&candidate) {
            candidate = mix64(candidate);
        }
        seeds.push(candidate);
    }
}

pub(super) fn scenario_seeds_from(
    scenario_raw: Option<&str>,
    variation_raw: Option<&str>,
) -> Result<ScenarioSeedPlan, GameplayHarnessConfigError> {
    if let Some(raw) = scenario_raw {
        if raw.trim().is_empty() {
            return Err(GameplayHarnessConfigError::EmptyScenarioSeedList);
        }
        let mut seeds = Vec::new();
        for (index, token) in raw.split(',').enumerate() {
            let seed = parse_seed(token)
                .ok_or(GameplayHarnessConfigError::InvalidScenarioSeed { index })?;
            seeds.push(seed);
        }
        return Ok(ScenarioSeedPlan {
            source: ScenarioSeedSource::Custom,
            seeds,
            anchor_seed_count: 0,
            variation_seed: None,
        });
    }

    let mut seeds = ANCHOR_SEEDS.to_vec();
    let variation_seed = resolve_variation_seed(variation_raw)?;
    append_organic_seeds(&mut seeds, variation_seed);
    Ok(ScenarioSeedPlan {
        source: ScenarioSeedSource::AnchorOrganic,
        seeds,
        anchor_seed_count: ANCHOR_SEEDS.len(),
        variation_seed: Some(variation_seed),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
            resolve_variation_seed(Some("nope")),
            Err(GameplayHarnessConfigError::InvalidVariationSeed)
        );
        assert_eq!(
            scenario_seeds_from(Some("1,nope,4"), None),
            Err(GameplayHarnessConfigError::InvalidScenarioSeed { index: 1 })
        );
        assert_eq!(
            scenario_seeds_from(Some(""), None),
            Err(GameplayHarnessConfigError::EmptyScenarioSeedList)
        );
    }

    #[test]
    fn custom_seed_list_is_exact_and_ignores_variation_seed() {
        let plan = scenario_seeds_from(Some("1, 0x2A,3"), Some("ignored"))
            .unwrap_or_else(|error| panic!("custom seed plan failed: {error:?}"));

        assert_eq!(plan.seeds(), [1, 42, 3]);
        assert_eq!(plan.source, ScenarioSeedSource::Custom);
        assert_eq!(plan.anchor_seed_count(), 0);
        assert_eq!(plan.organic_seed_count(), 0);
        assert_eq!(plan.custom_seed_count(), 3);
        assert_eq!(plan.variation_seed, None);
    }

    #[test]
    fn variation_root_replays_distinct_organic_scenarios_after_anchors() {
        let first = scenario_seeds_from(None, Some("0xBAD"))
            .unwrap_or_else(|error| panic!("first organic seed plan failed: {error:?}"));
        let second = scenario_seeds_from(None, Some("0xBAD"))
            .unwrap_or_else(|error| panic!("second organic seed plan failed: {error:?}"));

        assert_eq!(first, second);
        assert_eq!(first.source, ScenarioSeedSource::AnchorOrganic);
        assert_eq!(first.anchor_seed_count(), ANCHOR_SEEDS.len());
        assert_eq!(&first.seeds()[..ANCHOR_SEEDS.len()], ANCHOR_SEEDS);
        assert_eq!(first.organic_seed_count(), ORGANIC_SCENARIO_COUNT);
        assert_eq!(first.custom_seed_count(), 0);
        assert!(
            first.seeds()[ANCHOR_SEEDS.len()..]
                .iter()
                .all(|seed| !ANCHOR_SEEDS.contains(seed))
        );
        assert_eq!(first.variation_seed, Some(0xBAD));
    }
}
