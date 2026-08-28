//! Tests for the sibling load module; isolated so test-only edits do not invalidate production builds.

use super::*;

#[test]
fn kilogram_under_explicit_standard_gravity_rounds_up_conservatively() {
    let force = calculate_weight_force_ceiling(
        Mass::from_milligrams(1_000_000),
        Acceleration::from_micrometers_per_second_squared(9_806_650),
    );

    assert_eq!(force, Force::from_millinewtons(9_807));
}

#[test]
fn aggregate_weight_matches_single_mass_without_per_record_rounding() {
    let acceleration = Acceleration::from_micrometers_per_second_squared(9_806_650);
    let aggregate = match calculate_aggregate_weight_force_ceiling(
        AggregateMass::from_milligrams(2_000_000),
        acceleration,
    ) {
        Some(force) => force,
        None => panic!("aggregate weight unexpectedly overflowed"),
    };

    assert_eq!(aggregate, Force::from_millinewtons(19_614));
}

#[test]
fn zero_mass_produces_zero_force() {
    let acceleration = Acceleration::from_micrometers_per_second_squared(9_806_650);
    assert_eq!(
        calculate_weight_force_ceiling(Mass::ZERO, acceleration),
        Force::ZERO
    );
}
