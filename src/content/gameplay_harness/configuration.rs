//! Replayable gameplay-harness seed selection and environment configuration.

use std::env;
use std::time::{SystemTime, UNIX_EPOCH};

use super::seed::mix64;

const COVERAGE_SEEDS: [u64; 5] = [1, 4, 9, 19, 380];
const ORGANIC_SCENARIO_COUNT: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GameplayHarnessConfigError {
    InvalidVariationSeed,
    EmptyScenarioSeedList,
    InvalidScenarioSeed { index: usize },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ScenarioSeedPlan {
    pub(super) seeds: Vec<u64>,
    pub(super) coverage_seed_count: usize,
    pub(super) variation_seed: Option<u64>,
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

fn scenario_seeds_from(
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
            seeds,
            coverage_seed_count: 0,
            variation_seed: None,
        });
    }

    let mut seeds = COVERAGE_SEEDS.to_vec();
    let variation_seed = resolve_variation_seed(variation_raw)?;
    append_organic_seeds(&mut seeds, variation_seed);
    Ok(ScenarioSeedPlan {
        seeds,
        coverage_seed_count: COVERAGE_SEEDS.len(),
        variation_seed: Some(variation_seed),
    })
}

pub(super) fn scenario_seeds() -> Result<ScenarioSeedPlan, GameplayHarnessConfigError> {
    let scenario_raw = env::var("DEEP_HEARTH_GAMEPLAY_SEEDS").ok();
    let variation_raw = env::var("DEEP_HEARTH_GAMEPLAY_VARIATION_SEED").ok();
    scenario_seeds_from(scenario_raw.as_deref(), variation_raw.as_deref())
}

pub(super) fn configuration_contract_gaps() -> Vec<&'static str> {
    let checks = [
        ("decimal seed parsing", parse_seed("  42  ") == Some(42)),
        ("hex seed parsing", parse_seed("0x2A") == Some(42)),
        (
            "maximum decimal seed parsing",
            parse_seed("18446744073709551615") == Some(u64::MAX),
        ),
        (
            "maximum hexadecimal seed parsing",
            parse_seed("0xFFFFFFFFFFFFFFFF") == Some(u64::MAX),
        ),
        ("empty seed rejection", parse_seed("").is_none()),
        (
            "malformed seed rejection",
            parse_seed("not-a-seed").is_none(),
        ),
        (
            "variation seed override",
            resolve_variation_seed(Some("0xBAD")) == Ok(0xBAD),
        ),
        (
            "invalid variation seed rejection",
            resolve_variation_seed(Some("nope"))
                == Err(GameplayHarnessConfigError::InvalidVariationSeed),
        ),
        (
            "invalid custom seed entry rejection",
            scenario_seeds_from(Some("1,nope,4"), None)
                == Err(GameplayHarnessConfigError::InvalidScenarioSeed { index: 1 }),
        ),
        (
            "empty custom seed list rejection",
            scenario_seeds_from(Some(""), None)
                == Err(GameplayHarnessConfigError::EmptyScenarioSeedList),
        ),
    ];
    let mut gaps = checks
        .into_iter()
        .filter_map(|(name, passed)| (!passed).then_some(name))
        .collect::<Vec<_>>();

    match scenario_seeds_from(Some("1, 0x2A,3"), Some("ignored")) {
        Ok(plan)
            if plan.seeds == [1, 42, 3]
                && plan.coverage_seed_count == 0
                && plan.variation_seed.is_none() => {}
        Ok(_) | Err(_) => gaps.push("custom seed lists remain exact diagnostic inputs"),
    }
    match scenario_seeds_from(None, Some("0xBAD")) {
        Ok(plan)
            if plan.coverage_seed_count == COVERAGE_SEEDS.len()
                && plan.seeds[..plan.coverage_seed_count] == COVERAGE_SEEDS
                && plan.seeds.len() == COVERAGE_SEEDS.len() + ORGANIC_SCENARIO_COUNT
                && plan.variation_seed == Some(0xBAD)
                && plan.seeds[plan.coverage_seed_count..]
                    .iter()
                    .all(|seed| !COVERAGE_SEEDS.contains(seed)) => {}
        Ok(_) | Err(_) => {
            gaps.push("organic scenarios are replayable and excluded from maintained coverage")
        }
    }
    match (
        scenario_seeds_from(None, Some("0xBAD")),
        scenario_seeds_from(None, Some("0xBAD")),
    ) {
        (Ok(first), Ok(second)) if first == second => {}
        (Ok(_), Ok(_)) | (Ok(_), Err(_)) | (Err(_), Ok(_)) | (Err(_), Err(_)) => {
            gaps.push("variation root reproduces the same organic scenario set")
        }
    }

    gaps
}
