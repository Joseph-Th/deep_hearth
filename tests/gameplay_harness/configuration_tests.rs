//! Contract tests for gameplay seed configuration and replay planning.

use super::configuration::{
    GameplayHarnessConfigError, MAINTAINED_VARIATION_ROOT, MaintainedAnchor, ScenarioPlanMode,
    ScenarioSeedPlan, scenario_seeds_from,
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
) -> Result<ScenarioSeedPlan, GameplayHarnessConfigError> {
    scenario_seeds_from(mode, scenario_raw, variation_raw, MAINTAINED_VARIATION_ROOT)
}

#[test]
fn seed_configuration_rejects_invalid_inputs_with_exact_error_location() {
    assert_eq!(
        plan(ScenarioPlanMode::Explore, None, Some("nope")),
        Err(GameplayHarnessConfigError::InvalidVariationSeed)
    );
    assert_eq!(
        plan(ScenarioPlanMode::Gate, Some("1,nope,4"), None),
        Err(GameplayHarnessConfigError::InvalidScenarioSeed { index: 1 })
    );
    assert_eq!(
        plan(ScenarioPlanMode::Gate, Some(""), None),
        Err(GameplayHarnessConfigError::EmptyScenarioSeedList)
    );
}

#[test]
fn custom_world_seed_list_is_exact_and_replayable() {
    let plan = plan(ScenarioPlanMode::Gate, Some("1, 0x2A,3"), Some("ignored"))
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
    assert!(plan.cases().iter().all(|case| case.anchor.is_none()));
    let expected_replay = plan
        .cases()
        .iter()
        .map(|case| format!("0x{:016X}", case.world_seed))
        .collect::<Vec<_>>()
        .join(",");
    assert_eq!(plan.replay_label(), expected_replay);
}

#[test]
fn default_gate_runs_only_the_maintained_contract_set() {
    let plan = plan(ScenarioPlanMode::Gate, None, None)
        .unwrap_or_else(|error| panic!("default gate seed plan failed: {error:?}"));

    assert_eq!(plan.source_label(), "maintained");
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
    assert_eq!(plan.variation_seed_count(), 0);
    assert_eq!(plan.custom_seed_count(), 0);
    assert_eq!(plan.cases().len(), EXPECTED_MAINTAINED_ANCHORS.len());
    assert!(plan.cases().iter().all(|case| case.anchor.is_some()));
    assert_eq!(plan.variation_label(), "n/a");
    assert_eq!(plan.replay_label(), "maintained");
}

#[test]
fn gate_roots_vary_only_the_bounded_organic_case() {
    let first = scenario_seeds_from(ScenarioPlanMode::Gate, None, Some("0x1111"), 0x1111)
        .unwrap_or_else(|error| panic!("first gate-default plan failed: {error:?}"));
    let second = scenario_seeds_from(ScenarioPlanMode::Gate, None, Some("0xAAAA"), 0xAAAA)
        .unwrap_or_else(|error| panic!("second gate-default plan failed: {error:?}"));
    assert_eq!(first.anchor_seed_count(), EXPECTED_MAINTAINED_ANCHORS.len());
    assert_eq!(
        second.anchor_seed_count(),
        EXPECTED_MAINTAINED_ANCHORS.len()
    );
    assert_eq!(first.source_label(), "maintained+variation");
    assert_eq!(first.variation_seed_count(), 1);
    assert_eq!(first.variation_label(), "0x0000000000001111");
    assert_eq!(second.variation_label(), "0x000000000000AAAA");
    assert_eq!(
        &first.cases()[..EXPECTED_MAINTAINED_ANCHORS.len()],
        &second.cases()[..EXPECTED_MAINTAINED_ANCHORS.len()],
        "organic roots must not perturb maintained physical witnesses"
    );
    assert_ne!(
        first.cases()[EXPECTED_MAINTAINED_ANCHORS.len()],
        second.cases()[EXPECTED_MAINTAINED_ANCHORS.len()],
        "different replay roots must vary the supplemental gate case"
    );

    let exploratory = scenario_seeds_from(ScenarioPlanMode::Explore, None, None, 0x1111)
        .unwrap_or_else(|error| panic!("exploratory plan failed: {error:?}"));
    assert_eq!(exploratory.source_label(), "maintained+variation");
    assert!(
        exploratory.variation_seed_count() > first.variation_seed_count(),
        "exploration must be broader than a one-case explicit replay without pinning tuning"
    );
    assert_eq!(exploratory.variation_label(), "0x0000000000001111");
}

#[test]
fn explicit_world_root_replays_the_same_cases() {
    let first = plan(ScenarioPlanMode::Explore, None, Some("0xBAD"))
        .unwrap_or_else(|error| panic!("first variation seed plan failed: {error:?}"));
    let second = plan(ScenarioPlanMode::Explore, None, Some("0xBAD"))
        .unwrap_or_else(|error| panic!("second variation seed plan failed: {error:?}"));

    assert_eq!(first, second);
    assert_eq!(first.source_label(), "maintained+variation");
    assert_eq!(first.anchor_seed_count(), EXPECTED_MAINTAINED_ANCHORS.len());
    assert_eq!(
        first
            .cases()
            .iter()
            .filter_map(|case| case.anchor.map(|anchor| (anchor, case.world_seed)))
            .collect::<Vec<_>>(),
        EXPECTED_MAINTAINED_ANCHORS
    );
    assert!(
        first.variation_seed_count() > 0,
        "explicit exploratory roots must produce bounded organic variation"
    );
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
}
