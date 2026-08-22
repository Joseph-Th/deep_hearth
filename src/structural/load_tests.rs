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
fn pressure_area_conversion_is_exact_when_millinewton_aligned() {
    let force = calculate_pressure_force_ceiling(
        Pressure::from_pascals(2_000),
        Area::from_square_millimeters(500),
    );

    assert_eq!(force, Force::from_millinewtons(1_000));
}

#[test]
fn fractional_pressure_force_rounds_up_instead_of_erasing_load() {
    let force = calculate_pressure_force_ceiling(
        Pressure::from_pascals(1),
        Area::from_square_millimeters(1),
    );

    assert_eq!(force, Force::from_millinewtons(1));
}

#[test]
fn zero_source_quantities_produce_zero_force() {
    let acceleration = Acceleration::from_micrometers_per_second_squared(9_806_650);
    assert_eq!(
        calculate_weight_force_ceiling(Mass::ZERO, acceleration),
        Force::ZERO
    );
    assert_eq!(
        calculate_pressure_force_ceiling(Pressure::ZERO, Area::from_square_millimeters(1)),
        Force::ZERO
    );
}
