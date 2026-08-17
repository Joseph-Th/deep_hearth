//! Deterministic gameplay-harness seed selection and environment configuration.

use std::env;

const COVERAGE_SEEDS: [u64; 5] = [1, 4, 5, 23, 957];

/// Fixed default for the extra uncurated exploratory probe.
const DEFAULT_EXPLORATORY_SEED: u64 = 0xD33D_1A5E_BEEF_5EED;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GameplayHarnessConfigError {
    InvalidExploratorySeed,
    EmptyScenarioSeedList,
    InvalidScenarioSeed { index: usize },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ScenarioSeedPlan {
    pub(super) seeds: Vec<u64>,
    pub(super) coverage_seed_count: usize,
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

fn resolve_exploratory_seed(raw: Option<&str>) -> Result<u64, GameplayHarnessConfigError> {
    match raw {
        Some(text) => parse_seed(text).ok_or(GameplayHarnessConfigError::InvalidExploratorySeed),
        None => Ok(DEFAULT_EXPLORATORY_SEED),
    }
}

fn scenario_seeds_from(
    scenario_raw: Option<&str>,
    exploratory_raw: Option<&str>,
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
        });
    }

    let mut seeds = COVERAGE_SEEDS.to_vec();
    seeds.push(resolve_exploratory_seed(exploratory_raw)?);
    Ok(ScenarioSeedPlan {
        seeds,
        coverage_seed_count: COVERAGE_SEEDS.len(),
    })
}

pub(super) fn scenario_seeds() -> Result<ScenarioSeedPlan, GameplayHarnessConfigError> {
    let scenario_raw = env::var("DEEP_HEARTH_GAMEPLAY_SEEDS").ok();
    let exploratory_raw = env::var("DEEP_HEARTH_GAMEPLAY_EXPLORATORY_SEED").ok();
    scenario_seeds_from(scenario_raw.as_deref(), exploratory_raw.as_deref())
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
            "default exploratory seed",
            resolve_exploratory_seed(None) == Ok(DEFAULT_EXPLORATORY_SEED),
        ),
        (
            "exploratory seed override",
            resolve_exploratory_seed(Some("0xBAD")) == Ok(0xBAD),
        ),
        (
            "invalid exploratory seed rejection",
            resolve_exploratory_seed(Some("nope"))
                == Err(GameplayHarnessConfigError::InvalidExploratorySeed),
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
        Ok(plan) if plan.seeds == [1, 42, 3] && plan.coverage_seed_count == 0 => {}
        Ok(_) | Err(_) => gaps.push("custom seed lists remain exact diagnostic inputs"),
    }
    match scenario_seeds_from(None, Some("0xBAD")) {
        Ok(plan)
            if plan.coverage_seed_count == COVERAGE_SEEDS.len()
                && plan.seeds[..plan.coverage_seed_count] == COVERAGE_SEEDS
                && plan.seeds.last() == Some(&0xBAD) => {}
        Ok(_) | Err(_) => gaps.push("exploratory seed is excluded from maintained coverage"),
    }

    gaps
}
