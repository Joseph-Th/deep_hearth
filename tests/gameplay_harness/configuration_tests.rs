//! Contract tests for gameplay seed configuration and replay planning.

use super::configuration::{
    GameplayHarnessConfigError, MAINTAINED_BEHAVIOR_ROOT, MAINTAINED_VARIATION_ROOT,
    MaintainedAnchor, ScenarioPlanMode, ScenarioSeedPlan, scenario_seeds_from,
};

const EXPECTED_MAINTAINED_ANCHORS: [(MaintainedAnchor, u64); 7] = [
    (MaintainedAnchor::NormalBaseline, 1),
    (MaintainedAnchor::WarningMaintenance, 4),
    (MaintainedAnchor::CriticalMaintenance, 9),
    (MaintainedAnchor::ConditionPressure, 29),
    (MaintainedAnchor::AdaptiveEnergy, 19),
    (MaintainedAnchor::ManualRecovery, 380),
    (MaintainedAnchor::SurvivalRecovery, 0x1F65_DBFE_4A87_A054),
];

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
        plan(ScenarioPlanMode::Explore, None, Some("nope"), None),
        Err(GameplayHarnessConfigError::InvalidVariationSeed)
    );
    assert_eq!(
        plan(ScenarioPlanMode::Explore, None, None, Some("nope")),
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
    assert_eq!(plan.source_label(), "custom");
    assert_eq!(plan.anchor_seed_count(), 0);
    assert_eq!(plan.variation_seed_count(), 0);
    assert_eq!(plan.custom_seed_count(), 3);
    assert_eq!(plan.variation_label(), "n/a");
    assert_eq!(plan.behavior_label(), "0x000000000000BEEF");
    assert!(plan.cases().iter().all(|case| case.anchor.is_none()));
    assert!(
        plan.cases()
            .windows(2)
            .all(|pair| pair[0].behavior_seed != pair[1].behavior_seed)
    );
    let expected_replay = plan
        .cases()
        .iter()
        .map(|case| format!("0x{:016X}@0x{:016X}", case.world_seed, case.behavior_seed))
        .collect::<Vec<_>>()
        .join(",");
    assert_eq!(plan.replay_label(), expected_replay);
}

#[test]
fn default_gate_keeps_maintained_anchors_and_adds_one_replayable_variation() {
    let plan = plan(ScenarioPlanMode::Gate, None, None, None)
        .unwrap_or_else(|error| panic!("default gate seed plan failed: {error:?}"));

    assert_eq!(plan.source_label(), "anchor+variation");
    assert_eq!(
        MaintainedAnchor::ALL.map(|anchor| anchor.label()),
        [
            "normal-baseline",
            "warning-maintenance",
            "critical-maintenance",
            "condition-pressure",
            "adaptive-energy",
            "manual-recovery",
            "survival-recovery",
        ]
    );
    assert_eq!(
        plan.cases()
            .iter()
            .filter_map(|case| case.anchor.map(|anchor| (anchor, case.world_seed)))
            .collect::<Vec<_>>(),
        EXPECTED_MAINTAINED_ANCHORS
    );
    assert_eq!(plan.anchor_seed_count(), EXPECTED_MAINTAINED_ANCHORS.len());
    assert_eq!(plan.variation_seed_count(), 1);
    assert_eq!(plan.cases().len(), EXPECTED_MAINTAINED_ANCHORS.len() + 1);
    assert_eq!(
        plan.variation_label(),
        format!("0x{MAINTAINED_VARIATION_ROOT:016X}")
    );
    assert_eq!(plan.behavior_label(), "0x0000000000000001");
    assert!(
        plan.cases()[..EXPECTED_MAINTAINED_ANCHORS.len()]
            .iter()
            .all(|case| case.anchor.is_some())
    );
    assert!(
        plan.cases()
            .last()
            .is_some_and(|case| case.anchor.is_none())
    );
}

#[test]
fn explicit_gate_variation_root_adds_one_replay_case_without_changing_maintained_anchors() {
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

    assert_eq!(first.anchor_seed_count(), EXPECTED_MAINTAINED_ANCHORS.len());
    assert_eq!(
        second.anchor_seed_count(),
        EXPECTED_MAINTAINED_ANCHORS.len()
    );
    assert_eq!(first.source_label(), "anchor+variation");
    assert_eq!(first.variation_seed_count(), 1);
    assert_eq!(second.variation_seed_count(), 1);
    assert_eq!(
        &first.cases()[..EXPECTED_MAINTAINED_ANCHORS.len()],
        &second.cases()[..EXPECTED_MAINTAINED_ANCHORS.len()]
    );
    assert_ne!(first.cases().last(), second.cases().last());
    assert_eq!(first.variation_label(), "0x0000000000001111");
    assert_eq!(first.behavior_label(), "0x0000000000002222");
    assert_eq!(second.variation_label(), "0x000000000000AAAA");
    assert_eq!(second.behavior_label(), "0x000000000000BBBB");

    assert_eq!(
        plan(ScenarioPlanMode::Gate, None, None, Some("ignored")),
        Err(GameplayHarnessConfigError::InvalidBehaviorSeed),
        "gate behavior input is active because the supplemental organic case varies actor policy"
    );

    let exploratory =
        scenario_seeds_from(ScenarioPlanMode::Explore, None, None, None, 0x1111, 0x2222)
            .unwrap_or_else(|error| panic!("exploratory plan failed: {error:?}"));
    assert_eq!(exploratory.source_label(), "anchor+variation");
    assert_eq!(exploratory.variation_seed_count(), 4);
    assert_eq!(exploratory.variation_label(), "0x0000000000001111");
    assert_eq!(exploratory.behavior_label(), "0x0000000000002222");
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
    assert_eq!(first.source_label(), "anchor+variation");
    assert_eq!(first.anchor_seed_count(), EXPECTED_MAINTAINED_ANCHORS.len());
    assert_eq!(
        first
            .cases()
            .iter()
            .filter_map(|case| case.anchor.map(|anchor| (anchor, case.world_seed)))
            .collect::<Vec<_>>(),
        EXPECTED_MAINTAINED_ANCHORS
    );
    assert_eq!(first.variation_seed_count(), 4);
    assert_eq!(first.custom_seed_count(), 0);
    assert!(
        first
            .cases()
            .iter()
            .filter(|case| case.anchor.is_none())
            .all(|case| !EXPECTED_MAINTAINED_ANCHORS
                .iter()
                .any(|(_, world_seed)| *world_seed == case.world_seed))
    );
    assert_eq!(first.variation_label(), "0x0000000000000BAD");
    assert_eq!(first.behavior_label(), "0x000000000000CAFE");
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
