//! Tests for exact power integration, inverse duration calculation, and specific-energy scaling.

use super::*;

const fn twentieth_second_tick() -> PhysicalTickDuration {
    PhysicalTickDuration::from_microseconds(50_000)
}

#[test]
fn mass_specific_energy_scales_exactly_without_rounding() {
    assert_eq!(
        calculate_mass_specific_energy(
            Mass::from_milligrams(25),
            MassSpecificEnergy::from_nanojoules_per_milligram(40),
        ),
        Energy::from_nanojoules(1_000)
    );
}

#[test]
fn twentieth_second_power_integration_is_exact_for_one_microwatt() {
    let result = match integrate_power(
        Power::from_microwatts(1),
        TickSpan::new(1),
        twentieth_second_tick(),
        PowerRemainder::ZERO,
    ) {
        Ok(result) => result,
        Err(error) => panic!("power integration failed: {error}"),
    };

    assert_eq!(result.energy(), Energy::from_nanojoules(50));
    assert_eq!(result.remainder(), PowerRemainder::ZERO);
}

#[test]
fn fractional_tick_energy_is_preserved_across_repeated_steps() {
    let tick_duration = PhysicalTickDuration::from_microseconds(100_000);
    let mut remainder = PowerRemainder::ZERO;
    let mut accumulated = Energy::ZERO;
    for _ in 0..10 {
        let result = match integrate_power(
            Power::from_microwatts(1),
            TickSpan::new(1),
            tick_duration,
            remainder,
        ) {
            Ok(result) => result,
            Err(error) => panic!("power integration failed: {error}"),
        };
        accumulated = match accumulated.checked_add(result.energy()) {
            Some(value) => value,
            None => panic!("test energy accumulation overflowed"),
        };
        remainder = result.remainder();
    }

    assert_eq!(accumulated, Energy::from_nanojoules(1_000));
    assert_eq!(remainder, PowerRemainder::ZERO);
}

#[test]
fn duration_ceiling_returns_first_tick_that_meets_energy_requirement() {
    let tick_duration = twentieth_second_tick();
    let required = Energy::from_nanojoules(51);
    let duration = match calculate_power_duration_ceiling(
        Power::from_microwatts(1),
        required,
        tick_duration,
    ) {
        Ok(duration) => duration,
        Err(error) => panic!("duration calculation failed: {error}"),
    };

    assert_eq!(duration, TickSpan::new(2));
    let one_tick = match integrate_power(
        Power::from_microwatts(1),
        TickSpan::new(1),
        tick_duration,
        PowerRemainder::ZERO,
    ) {
        Ok(result) => result.energy(),
        Err(error) => panic!("one-tick integration failed: {error}"),
    };
    let two_ticks = match integrate_power(
        Power::from_microwatts(1),
        duration,
        tick_duration,
        PowerRemainder::ZERO,
    ) {
        Ok(result) => result.energy(),
        Err(error) => panic!("two-tick integration failed: {error}"),
    };
    assert!(one_tick < required);
    assert!(two_ticks >= required);
}

#[test]
fn duration_ceiling_rejects_nonzero_energy_at_zero_power() {
    assert_eq!(
        calculate_power_duration_ceiling(
            Power::ZERO,
            Energy::from_nanojoules(1),
            twentieth_second_tick(),
        ),
        Err(PowerDurationError::ZeroPower)
    );
}

#[test]
fn duration_ceiling_handles_maximum_authoritative_values_without_overflow() {
    let duration = match calculate_power_duration_ceiling(
        Power::from_picowatts(u128::MAX),
        Energy::from_nanojoules(u128::MAX),
        PhysicalTickDuration::from_microseconds(1_000_000),
    ) {
        Ok(duration) => duration,
        Err(error) => panic!("maximum-value duration calculation failed: {error}"),
    };

    assert_eq!(duration, TickSpan::new(1_000));
}

#[test]
fn duration_ceiling_reports_when_u64_tick_range_is_insufficient() {
    assert_eq!(
        calculate_power_duration_ceiling(
            Power::from_picowatts(1),
            Energy::from_nanojoules(u128::MAX),
            PhysicalTickDuration::from_microseconds(1),
        ),
        Err(PowerDurationError::DurationOverflow)
    );
}
