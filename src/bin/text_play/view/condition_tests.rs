//! Regression tests for condition readouts.

#[test]
fn nonzero_vitality_and_reserves_never_display_as_exhausted() {
    use super::{reserve_label, vitality_label};
    assert_eq!(vitality_label(0), "0");
    assert_eq!(vitality_label(1), "<0.01");
    assert_eq!(vitality_label(9_100), "0.91");
    assert_eq!(vitality_label(10_000), "1");
    assert_eq!(vitality_label(1_000_000), "100");
    assert_eq!(reserve_label(0, false), "<1");
    assert_eq!(reserve_label(0, true), "0");
    assert_eq!(reserve_label(78, false), "78");
}

#[test]
fn signed_foot_elevation_is_correct_below_zero() {
    assert_eq!(super::meters_from_nanometers(2_225_000_000), "2.225");
    assert_eq!(super::meters_from_nanometers(-500_000_000), "-0.500");
    assert_eq!(super::meters_from_nanometers(-1_500_000_000), "-1.500");
    assert!(super::meters_from_nanometers(i64::MIN).starts_with('-'));
}
