//! Workshop-target tests for shared replay-seed parsing and focused-probe planning.

use super::focused_seeds::{
    FOCUSED_VARIATION_COUNT, FocusedProbeSeedError, focused_probe_seeds_from,
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
fn focused_default_keeps_anchor_and_adds_a_tiny_replayable_variation_sample() {
    let first = focused_probe_seeds_from(None, None, 0x1111, 0x2222, 0x3333)
        .unwrap_or_else(|error| panic!("first focused probe plan failed: {error:?}"));
    let second = focused_probe_seeds_from(None, None, 0x1111, 0x2222, 0x3333)
        .unwrap_or_else(|error| panic!("second focused probe plan failed: {error:?}"));

    assert_eq!(first, second);
    assert_eq!(first.len(), 1 + FOCUSED_VARIATION_COUNT);
    assert_eq!(first[0], 0x1111);
    assert_ne!(first[1], first[0]);
    assert_ne!(first[2], first[0]);
    assert_ne!(first[2], first[1]);
}

#[test]
fn focused_generated_variation_changes_when_the_default_root_changes() {
    let first = focused_probe_seeds_from(None, None, 0x1111, 0x2222, 0x3333)
        .unwrap_or_else(|error| panic!("first focused generated plan failed: {error:?}"));
    let second = focused_probe_seeds_from(None, None, 0x1111, 0x2222, 0x4444)
        .unwrap_or_else(|error| panic!("second focused generated plan failed: {error:?}"));

    assert_eq!(first[0], second[0]);
    assert_ne!(first[1], second[1]);
    assert_ne!(first[2], second[2]);
}

#[test]
fn focused_explicit_variation_root_replays_exactly() {
    let first = focused_probe_seeds_from(None, Some("0xBAD"), 0x1111, 0x2222, 0x3333)
        .unwrap_or_else(|error| panic!("first focused replay plan failed: {error:?}"));
    let second = focused_probe_seeds_from(None, Some("0xBAD"), 0x1111, 0x2222, 0x4444)
        .unwrap_or_else(|error| panic!("second focused replay plan failed: {error:?}"));

    assert_eq!(first, second);
}

#[test]
fn focused_explicit_seed_list_is_exact_and_invalid_variation_is_rejected() {
    assert_eq!(
        focused_probe_seeds_from(Some("1,0x2A,3"), Some("ignored"), 1, 2, 3),
        Ok(vec![1, 42, 3])
    );
    assert_eq!(
        focused_probe_seeds_from(None, Some("nope"), 1, 2, 3),
        Err(FocusedProbeSeedError::InvalidVariationSeed)
    );
}
