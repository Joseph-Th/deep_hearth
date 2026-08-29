//! Contract tests for gameplay seed configuration and replay planning.

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
fn default_gate_keeps_maintained_anchors_and_adds_one_organic_case() {
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
fn gate_and_explore_use_replay_roots_with_different_bounded_sample_sizes() {
    let first = scenario_seeds_from(
        ScenarioPlanMode::Gate,
        None,
        Some("0x1111"),
        Some("0x2222"),
        0x1111,
        0x2222,
    )
    .unwrap_or_else(|error| panic!("first gate-default plan failed: {error:?}"));
    let second = scenario_seeds_from(
        ScenarioPlanMode::Gate,
        None,
        Some("0xAAAA"),
        Some("0xBBBB"),
        0xAAAA,
        0xBBBB,
    )
    .unwrap_or_else(|error| panic!("second gate-default plan failed: {error:?}"));

    assert_eq!(first.anchor_seed_count(), MAINTAINED_ANCHORS.len());
    assert_eq!(second.anchor_seed_count(), MAINTAINED_ANCHORS.len());
    assert_eq!(first.variation_seed_count(), GATE_VARIATION_SCENARIO_COUNT);
    assert_eq!(second.variation_seed_count(), GATE_VARIATION_SCENARIO_COUNT);
    assert_ne!(first.cases().last(), second.cases().last());
    assert_eq!(first.variation_seed, Some(0x1111));
    assert_eq!(first.behavior_seed_root, 0x2222);

    let exploratory =
        scenario_seeds_from(ScenarioPlanMode::Explore, None, None, None, 0x1111, 0x2222)
            .unwrap_or_else(|error| panic!("exploratory plan failed: {error:?}"));
    assert_eq!(exploratory.source, ScenarioSeedSource::AnchorVariation);
    assert_eq!(
        exploratory.variation_seed_count(),
        EXPLORATORY_VARIATION_SCENARIO_COUNT
    );
    assert_eq!(exploratory.variation_seed, Some(0x1111));
    assert_eq!(exploratory.behavior_seed_root, 0x2222);
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
