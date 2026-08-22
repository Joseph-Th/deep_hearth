//! Tests for the sibling quantity module; isolated so test-only edits do not invalidate production builds.

use super::*;

#[test]
fn mass_arithmetic_rejects_overflow_and_underflow() {
    assert_eq!(
        Mass::from_milligrams(7).checked_add(Mass::from_milligrams(5)),
        Some(Mass::from_milligrams(12))
    );
    assert_eq!(
        Mass::from_milligrams(7).checked_sub(Mass::from_milligrams(5)),
        Some(Mass::from_milligrams(2))
    );
    assert_eq!(
        Mass::from_milligrams(u64::MAX).checked_add(Mass::from_milligrams(1)),
        None
    );
    assert_eq!(
        Mass::from_milligrams(1).checked_sub(Mass::from_milligrams(2)),
        None
    );
}

#[test]
fn aggregate_volume_accumulates_beyond_single_store_range() {
    let largest_store = AggregateVolume::from_volume(Volume::from_microliters(u64::MAX));

    assert_eq!(
        largest_store.checked_add(largest_store),
        Some(AggregateVolume::from_microliters(u128::from(u64::MAX) * 2))
    );
}

#[test]
fn absolute_temperature_arithmetic_cannot_cross_zero_or_overflow() {
    let temperature = Temperature::from_millikelvin(293_150);

    assert_eq!(
        temperature.checked_add_millikelvin(1_000),
        Some(Temperature::from_millikelvin(294_150))
    );
    assert_eq!(
        temperature.checked_sub_millikelvin(1_000),
        Some(Temperature::from_millikelvin(292_150))
    );
    assert_eq!(Temperature::ZERO.checked_sub_millikelvin(1), None);
    assert_eq!(
        Temperature::from_millikelvin(u32::MAX).checked_add_millikelvin(1),
        None
    );
}

#[test]
fn energy_arithmetic_is_exact_and_checked() {
    let first = Energy::from_nanojoules(9);
    let second = Energy::from_nanojoules(4);

    assert_eq!(first.checked_add(second), Some(Energy::from_nanojoules(13)));
    assert_eq!(first.checked_sub(second), Some(Energy::from_nanojoules(5)));
    assert_eq!(second.checked_sub(first), None);
    assert_eq!(
        Energy::from_nanojoules(u128::MAX).checked_add(Energy::from_nanojoules(1)),
        None
    );
}

#[test]
fn aggregate_mass_accumulates_beyond_single_record_range() {
    let largest_record = AggregateMass::from_mass(Mass::from_milligrams(u64::MAX));

    assert_eq!(
        largest_record.checked_add(largest_record),
        Some(AggregateMass::from_milligrams(u128::from(u64::MAX) * 2))
    );
}

#[test]
fn physical_rate_and_electrical_quantities_use_explicit_units() {
    assert_eq!(Pressure::from_pascals(101_325).pascals(), 101_325);
    assert_eq!(Area::from_square_millimeters(250).square_millimeters(), 250);
    assert_eq!(Length::from_micrometers(2_500).micrometers(), 2_500);
    assert_eq!(
        Acceleration::from_micrometers_per_second_squared(9_806_650)
            .micrometers_per_second_squared(),
        9_806_650
    );
    assert_eq!(Force::from_millinewtons(4_000).millinewtons(), 4_000);
    assert_eq!(Power::from_microwatts(3).picowatts(), 3_000_000);
    assert_eq!(
        Torque::from_micronewton_meters(2_000_000).micronewton_meters(),
        2_000_000
    );
    assert_eq!(
        AngularSpeed::from_microradians_per_second(3_000_000).microradians_per_second(),
        3_000_000
    );
    assert_eq!(ElectricPotential::from_microvolts(12).microvolts(), 12);
    assert_eq!(ElectricCurrent::from_microamperes(5).microamperes(), 5);
    assert_eq!(ElectricalResistance::from_microohms(7).microohms(), 7);
    assert_eq!(Volume::from_microliters(9).microliters(), 9);
    assert_eq!(
        MassSpecificEnergy::from_nanojoules_per_milligram(17).nanojoules_per_milligram(),
        17
    );
    assert_eq!(
        MassFlow::from_milligrams_per_second(13).milligrams_per_second(),
        13
    );
    assert_eq!(
        VolumetricFlow::from_microliters_per_second(11).microliters_per_second(),
        11
    );
}
