//! Contract tests for checked physical quantities.

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
fn precise_energy_preserves_fractional_carry_borrow_and_narrowing() {
    let first = PreciseEnergy::from_nanojoules_with_femtojoule_remainder(4, 750_000)
        .unwrap_or_else(|| panic!("precise-energy fixture must be normalized"));
    let second = PreciseEnergy::from_nanojoules_with_femtojoule_remainder(2, 500_000)
        .unwrap_or_else(|| panic!("precise-energy fixture must be normalized"));
    let expected = PreciseEnergy::from_nanojoules_with_femtojoule_remainder(7, 250_000)
        .unwrap_or_else(|| panic!("precise-energy expectation must be normalized"));

    assert_eq!(first.checked_add(second), Some(expected));
    assert_eq!(expected.checked_sub(second), Some(first));
    assert_eq!(first.whole_nanojoules(), None);
    assert_eq!(
        PreciseEnergy::from_energy(Energy::from_nanojoules(9)).whole_nanojoules(),
        Some(Energy::from_nanojoules(9))
    );
    assert_eq!(
        PreciseEnergy::from_nanojoules_with_femtojoule_remainder(1, 1_000_000),
        None,
        "precise energy must reject an unnormalized remainder"
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
fn physical_quantities_use_explicit_units() {
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
    assert_eq!(Volume::from_microliters(9).microliters(), 9);
    assert_eq!(
        MassSpecificEnergy::from_nanojoules_per_milligram(17).nanojoules_per_milligram(),
        17
    );
    assert_eq!(
        MassFlow::from_milligrams_per_second(13).milligrams_per_second(),
        13
    );
}
