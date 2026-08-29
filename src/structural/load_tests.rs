//! Contract tests for structural load representation.

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
fn fractional_mass_rounds_only_at_the_force_boundary() {
    let acceleration = Acceleration::from_micrometers_per_second_squared(9_806_650);
    let force = calculate_fractional_milligram_weight_force_ceiling(101_250, 1_000, acceleration)
        .unwrap_or_else(|| panic!("fractional structural weight unexpectedly overflowed"));

    assert_eq!(force, Force::from_millinewtons(1));
    assert_eq!(
        calculate_weight_force_ceiling(Mass::from_milligrams(102), acceleration),
        Force::from_millinewtons(2),
        "rounding 101.25 mg to 102 mg first demonstrates the double-ceiling error this helper avoids"
    );
}

#[test]
fn denominator_one_matches_aggregate_whole_milligram_weight() {
    let acceleration = Acceleration::from_micrometers_per_second_squared(9_806_650);
    let fractional =
        calculate_fractional_milligram_weight_force_ceiling(2_000_000, 1, acceleration)
            .unwrap_or_else(|| panic!("whole-mass rational weight unexpectedly overflowed"));
    let aggregate = calculate_aggregate_weight_force_ceiling(
        AggregateMass::from_milligrams(2_000_000),
        acceleration,
    )
    .unwrap_or_else(|| panic!("aggregate comparison weight unexpectedly overflowed"));

    assert_eq!(fractional, aggregate);
}

#[test]
fn zero_mass_produces_zero_force() {
    let acceleration = Acceleration::from_micrometers_per_second_squared(9_806_650);
    assert_eq!(
        calculate_weight_force_ceiling(Mass::ZERO, acceleration),
        Force::ZERO
    );
}
