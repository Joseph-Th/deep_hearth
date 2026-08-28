//! Workshop-target tests for shared replay-seed parsing and focused-probe planning.

use super::focused_seeds::{
    EXPLORATORY_VARIATION_COUNT, FocusedProbeRole, FocusedProbeSeedError, GATE_VARIATION_COUNT,
    focused_probe_cases_from,
};
use super::seed_input::{SeedListError, parse_seed, parse_seed_list};

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

    assert_eq!(&first[..2], &second[..2]);
    assert_ne!(&first[2..], &second[2..]);
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
