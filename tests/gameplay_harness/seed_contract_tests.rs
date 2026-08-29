//! Workshop-target tests for shared replay-seed parsing and focused-probe planning.

use super::focused_runner::probe_uses_actor_behavior;
use super::focused_seeds::{
    EXPLORATORY_VARIATION_COUNT, FocusedProbeRole, FocusedProbeSeedError, FocusedProbeSeedPlan,
    GATE_VARIATION_COUNT, focused_probe_cases_from as build_focused_probe_cases,
};
use super::seed_input::{SeedListError, parse_seed, parse_seed_list};

fn focused_probe_cases_from(
    variation_count: usize,
    scenario_raw: Option<&str>,
    variation_raw: Option<&str>,
    maintained_seed: u64,
    maintained_coverage_seeds: &[u64],
    probe_salt: u64,
    default_variation_root: u64,
) -> Result<Vec<super::focused_seeds::FocusedProbeCase>, FocusedProbeSeedError> {
    build_focused_probe_cases(FocusedProbeSeedPlan {
        variation_count,
        scenario_raw,
        variation_raw,
        behavior_raw: None,
        maintained_seed,
        maintained_coverage_seeds,
        probe_salt,
        default_variation_root,
        default_behavior_root: Some(0xB3A4_7102_5EED_2026),
    })
}

#[test]
fn seed_parser_accepts_decimal_hex_and_u64_boundaries() {
    assert_eq!(parse_seed("  42  "), Some(42));
    assert_eq!(parse_seed("0x2A"), Some(42));
    assert_eq!(parse_seed("18446744073709551615"), Some(u64::MAX));
    assert_eq!(parse_seed("0xFFFFFFFFFFFFFFFF"), Some(u64::MAX));
    assert_eq!(parse_seed(""), None);
    assert_eq!(parse_seed("not-a-seed"), None);
}

#[test]
fn seed_list_reports_empty_and_exact_invalid_position() {
    assert_eq!(parse_seed_list(""), Err(SeedListError::Empty));
    assert_eq!(
        parse_seed_list("1,nope,4"),
        Err(SeedListError::Invalid { index: 1 })
    );
    assert_eq!(parse_seed_list("1, 0x2A,3"), Ok(vec![1, 42, 3]));
}

#[test]
fn focused_gate_adds_one_replayable_organic_case() {
    let first = focused_probe_cases_from(
        GATE_VARIATION_COUNT,
        None,
        None,
        0x1111,
        &[0xAAAA, 0xBBBB],
        0x2222,
        0x3333,
    )
    .unwrap_or_else(|error| panic!("first focused probe plan failed: {error:?}"));
    let second = focused_probe_cases_from(
        GATE_VARIATION_COUNT,
        None,
        None,
        0x1111,
        &[0xAAAA, 0xBBBB],
        0x2222,
        0x4444,
    )
    .unwrap_or_else(|error| panic!("second focused probe plan failed: {error:?}"));

    assert_eq!(first.len(), 3 + GATE_VARIATION_COUNT);
    assert_eq!(first[0].seed(), 0x1111);
    assert_eq!(first[0].role(), FocusedProbeRole::MaintainedAnchor);
    assert_eq!(first[1].seed(), 0xAAAA);
    assert_eq!(first[1].role(), FocusedProbeRole::MaintainedCoverage);
    assert_eq!(first[2].seed(), 0xBBBB);
    assert_eq!(first[2].role(), FocusedProbeRole::MaintainedCoverage);
    assert_eq!(first[3].role(), FocusedProbeRole::OrganicVariation);
    assert_eq!(&first[..3], &second[..3]);
    assert_ne!(first[3].seed(), second[3].seed());
}

#[test]
fn focused_explore_adds_a_tiny_replayable_variation_sample() {
    let first = focused_probe_cases_from(
        EXPLORATORY_VARIATION_COUNT,
        None,
        None,
        0x1111,
        &[0xAAAA],
        0x2222,
        0x3333,
    )
    .unwrap_or_else(|error| panic!("first focused generated plan failed: {error:?}"));
    let second = focused_probe_cases_from(
        EXPLORATORY_VARIATION_COUNT,
        None,
        None,
        0x1111,
        &[0xAAAA],
        0x2222,
        0x4444,
    )
    .unwrap_or_else(|error| panic!("second focused generated plan failed: {error:?}"));

    assert_eq!(first.len(), 2 + EXPLORATORY_VARIATION_COUNT);
    assert!(
        first[2..]
            .iter()
            .all(|case| case.role() == FocusedProbeRole::OrganicVariation)
    );
    assert_eq!(&first[..2], &second[..2]);
    assert_ne!(first[2].seed(), second[2].seed());
    assert_eq!(
        first
            .iter()
            .map(|case| case.seed())
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        first.len(),
        "focused exploratory planning must keep maintained and generated worlds distinct"
    );
}

#[test]
fn focused_probe_salt_partitions_generated_variation_between_concerns() {
    let first = focused_probe_cases_from(
        EXPLORATORY_VARIATION_COUNT,
        None,
        None,
        0x1111,
        &[0xAAAA],
        0x2222,
        0x3333,
    )
    .unwrap_or_else(|error| panic!("first focused salted plan failed: {error:?}"));
    let second = focused_probe_cases_from(
        EXPLORATORY_VARIATION_COUNT,
        None,
        None,
        0x1111,
        &[0xAAAA],
        0x4444,
        0x3333,
    )
    .unwrap_or_else(|error| panic!("second focused salted plan failed: {error:?}"));

    assert_eq!(
        first[..2]
            .iter()
            .map(|case| (case.seed(), case.role()))
            .collect::<Vec<_>>(),
        second[..2]
            .iter()
            .map(|case| (case.seed(), case.role()))
            .collect::<Vec<_>>(),
        "probe salt must not change maintained physical worlds"
    );
    assert_ne!(
        first[..2]
            .iter()
            .map(|case| case.behavior_seed())
            .collect::<Vec<_>>(),
        second[..2]
            .iter()
            .map(|case| case.behavior_seed())
            .collect::<Vec<_>>(),
        "probe salt must keep maintained actor-policy streams concern-specific"
    );
    assert_ne!(
        first[2..]
            .iter()
            .map(|case| case.seed())
            .collect::<Vec<_>>(),
        second[2..]
            .iter()
            .map(|case| case.seed())
            .collect::<Vec<_>>(),
        "probe salt must partition generated physical variation between concerns"
    );
}

#[test]
fn focused_explicit_variation_root_replays_exactly() {
    let first = focused_probe_cases_from(
        EXPLORATORY_VARIATION_COUNT,
        None,
        Some("0xBAD"),
        0x1111,
        &[0xAAAA],
        0x2222,
        0x3333,
    )
    .unwrap_or_else(|error| panic!("first focused replay plan failed: {error:?}"));
    let second = focused_probe_cases_from(
        EXPLORATORY_VARIATION_COUNT,
        None,
        Some("0xBAD"),
        0x1111,
        &[0xAAAA],
        0x2222,
        0x4444,
    )
    .unwrap_or_else(|error| panic!("second focused replay plan failed: {error:?}"));

    assert_eq!(first, second);
}

#[test]
fn focused_explicit_seed_list_is_exact_and_invalid_variation_is_rejected() {
    let explicit = focused_probe_cases_from(0, Some("1,0x2A,3"), Some("ignored"), 1, &[9], 2, 3)
        .unwrap_or_else(|error| panic!("explicit replay cases failed: {error:?}"));
    assert_eq!(
        explicit.iter().map(|case| case.seed()).collect::<Vec<_>>(),
        vec![1, 42, 3]
    );
    assert!(
        explicit
            .iter()
            .all(|case| case.role() == FocusedProbeRole::ExplicitReplay)
    );
    assert_eq!(
        focused_probe_cases_from(
            EXPLORATORY_VARIATION_COUNT,
            None,
            Some("nope"),
            1,
            &[9],
            2,
            3,
        ),
        Err(FocusedProbeSeedError::InvalidVariationSeed)
    );
}

#[test]
fn focused_world_and_behavior_variation_are_independent_and_replayable() {
    let first = build_focused_probe_cases(FocusedProbeSeedPlan {
        variation_count: GATE_VARIATION_COUNT,
        scenario_raw: None,
        variation_raw: Some("0x1111"),
        behavior_raw: Some("0xAAAA"),
        maintained_seed: 0x1234,
        maintained_coverage_seeds: &[],
        probe_salt: 0x5678,
        default_variation_root: 0,
        default_behavior_root: Some(0),
    })
    .unwrap_or_else(|error| panic!("first independent focused plan failed: {error:?}"));
    let different_behavior = build_focused_probe_cases(FocusedProbeSeedPlan {
        variation_count: GATE_VARIATION_COUNT,
        scenario_raw: None,
        variation_raw: Some("0x1111"),
        behavior_raw: Some("0xBBBB"),
        maintained_seed: 0x1234,
        maintained_coverage_seeds: &[],
        probe_salt: 0x5678,
        default_variation_root: 0,
        default_behavior_root: Some(0),
    })
    .unwrap_or_else(|error| panic!("behavior-varied focused plan failed: {error:?}"));
    let different_world = build_focused_probe_cases(FocusedProbeSeedPlan {
        variation_count: GATE_VARIATION_COUNT,
        scenario_raw: None,
        variation_raw: Some("0x2222"),
        behavior_raw: Some("0xAAAA"),
        maintained_seed: 0x1234,
        maintained_coverage_seeds: &[],
        probe_salt: 0x5678,
        default_variation_root: 0,
        default_behavior_root: Some(0),
    })
    .unwrap_or_else(|error| panic!("world-varied focused plan failed: {error:?}"));

    assert_eq!(first[0], different_behavior[0]);
    assert_eq!(first[0], different_world[0]);
    assert_eq!(first[1].seed(), different_behavior[1].seed());
    assert_ne!(
        first[1].behavior_seed(),
        different_behavior[1].behavior_seed()
    );
    assert_ne!(first[1].seed(), different_world[1].seed());
    assert_eq!(first[1].behavior_seed(), different_world[1].behavior_seed());
}

#[test]
fn focused_behavior_root_is_validated_even_for_explicit_world_replay() {
    assert_eq!(
        build_focused_probe_cases(FocusedProbeSeedPlan {
            variation_count: 0,
            scenario_raw: Some("1"),
            variation_raw: None,
            behavior_raw: Some("nope"),
            maintained_seed: 1,
            maintained_coverage_seeds: &[],
            probe_salt: 2,
            default_variation_root: 3,
            default_behavior_root: Some(4),
        }),
        Err(FocusedProbeSeedError::InvalidBehaviorSeed)
    );
}

#[test]
fn focused_behavior_channel_exists_only_for_probes_with_actor_policy() {
    assert!(probe_uses_actor_behavior("survival-provisioning"));
    assert!(probe_uses_actor_behavior("primitive-progression"));
    assert!(!probe_uses_actor_behavior("ore-preparation"));
    assert!(!probe_uses_actor_behavior("foundry"));
}

#[test]
fn focused_plan_without_actor_channel_contains_no_behavior_seed() {
    let cases = build_focused_probe_cases(FocusedProbeSeedPlan {
        variation_count: GATE_VARIATION_COUNT,
        scenario_raw: None,
        variation_raw: Some("0x1111"),
        behavior_raw: None,
        maintained_seed: 0x1234,
        maintained_coverage_seeds: &[0x5678],
        probe_salt: 0x9ABC,
        default_variation_root: 0,
        default_behavior_root: None,
    })
    .unwrap_or_else(|error| panic!("no-actor focused plan failed: {error:?}"));

    assert!(cases.iter().all(|case| case.behavior_seed().is_none()));
}

#[test]
fn exploratory_actor_cases_stratify_one_behavior_bit_without_changing_worlds() {
    let cases = build_focused_probe_cases(FocusedProbeSeedPlan {
        variation_count: EXPLORATORY_VARIATION_COUNT,
        scenario_raw: None,
        variation_raw: Some("0x1111"),
        behavior_raw: Some("0x2222"),
        maintained_seed: 0x1234,
        maintained_coverage_seeds: &[],
        probe_salt: 0x5678,
        default_variation_root: 0,
        default_behavior_root: Some(0),
    })
    .unwrap_or_else(|error| panic!("stratified focused plan failed: {error:?}"));
    let organic = cases
        .iter()
        .filter(|case| case.role() == FocusedProbeRole::OrganicVariation)
        .copied()
        .collect::<Vec<_>>();

    assert_eq!(organic.len(), EXPLORATORY_VARIATION_COUNT);
    assert_ne!(organic[0].seed(), organic[1].seed());
    assert_ne!(
        organic[0]
            .behavior_seed()
            .unwrap_or_else(|| panic!("first organic actor seed missing"))
            & 1,
        organic[1]
            .behavior_seed()
            .unwrap_or_else(|| panic!("second organic actor seed missing"))
            & 1,
        "two-case exploratory actor sampling must span both binary behavior strata"
    );
}
