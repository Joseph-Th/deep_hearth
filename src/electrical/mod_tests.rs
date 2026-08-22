//! Tests for the sibling mod module; isolated so test-only edits do not invalidate production builds.

use super::*;

#[test]
fn household_scale_voltage_and_current_produce_exact_power() {
    let power = calculate_electrical_power(
        ElectricPotential::from_microvolts(230_000_000),
        ElectricCurrent::from_microamperes(16_000_000),
    );

    assert_eq!(power, Power::from_picowatts(3_680_000_000_000_000));
}

#[test]
fn resistive_voltage_drop_preserves_sub_microvolt_remainder() {
    let drop = match calculate_resistive_voltage_drop(
        ElectricCurrent::from_microamperes(2_500),
        ElectricalResistance::from_microohms(1_500),
    ) {
        Ok(drop) => drop,
        Err(error) => panic!("voltage-drop calculation failed: {error}"),
    };

    assert_eq!(drop.potential(), ElectricPotential::from_microvolts(3));
    assert_eq!(drop.remainder().picovolts(), 750_000);
    assert_eq!(drop.remainder().validate(), Ok(()));
}

#[test]
fn potential_remainder_deserialization_rejects_one_microvolt_or_more() {
    let result: Result<PotentialRemainderPicovolts, _> = serde_json::from_str("1000000");
    assert!(result.is_err());
}
