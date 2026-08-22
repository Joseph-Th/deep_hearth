//! Tests for the sibling mod module; isolated so test-only edits do not invalidate production builds.

use super::*;

fn efficiency(parts_per_million: u32) -> MechanicalEfficiency {
    match MechanicalEfficiency::new(parts_per_million) {
        Ok(efficiency) => efficiency,
        Err(error) => panic!("mechanical efficiency fixture failed: {error}"),
    }
}

#[test]
fn torque_times_speed_produces_exact_picowatt_power() {
    let point = match validate_rotational_operating_point(
        RotationalLimits::new(
            Torque::from_micronewton_meters(3_000_000),
            AngularSpeed::from_microradians_per_second(4_000_000),
            Power::from_picowatts(10_000_000_000_000),
        ),
        Torque::from_micronewton_meters(2_000_000),
        AngularSpeed::from_microradians_per_second(3_000_000),
    ) {
        Ok(point) => point,
        Err(error) => panic!("rotational point unexpectedly rejected: {error}"),
    };

    assert_eq!(point.power(), Power::from_picowatts(6_000_000_000_000));
    assert_eq!(point.torque(), Torque::from_micronewton_meters(2_000_000));
    assert_eq!(
        point.angular_speed(),
        AngularSpeed::from_microradians_per_second(3_000_000)
    );
}

#[test]
fn combined_power_limit_can_bind_before_torque_or_speed_limits() {
    let limits = RotationalLimits::new(
        Torque::from_micronewton_meters(10),
        AngularSpeed::from_microradians_per_second(10),
        Power::from_picowatts(50),
    );

    assert_eq!(
        validate_rotational_operating_point(
            limits,
            Torque::from_micronewton_meters(8),
            AngularSpeed::from_microradians_per_second(8),
        ),
        Err(RotationalOperatingPointError::PowerExceeded {
            requested: Power::from_picowatts(64),
            maximum: Power::from_picowatts(50),
        })
    );
}

#[test]
fn torque_and_speed_limits_report_the_specific_constraint() {
    let limits = RotationalLimits::new(
        Torque::from_micronewton_meters(10),
        AngularSpeed::from_microradians_per_second(20),
        Power::from_picowatts(1_000),
    );

    assert!(matches!(
        validate_rotational_operating_point(
            limits,
            Torque::from_micronewton_meters(11),
            AngularSpeed::from_microradians_per_second(1),
        ),
        Err(RotationalOperatingPointError::TorqueExceeded {
            requested: _requested,
            maximum: _maximum,
        })
    ));
    assert!(matches!(
        validate_rotational_operating_point(
            limits,
            Torque::from_micronewton_meters(1),
            AngularSpeed::from_microradians_per_second(21),
        ),
        Err(RotationalOperatingPointError::AngularSpeedExceeded {
            requested: _requested,
            maximum: _maximum,
        })
    ));
}

#[test]
fn efficiency_preserves_input_as_useful_output_plus_explicit_loss() {
    let input = Power::from_picowatts(6_000_000_000_000);
    let transfer = apply_mechanical_efficiency(input, efficiency(850_000));

    assert_eq!(transfer.input(), input);
    assert_eq!(transfer.output(), Power::from_picowatts(5_100_000_000_000));
    assert_eq!(transfer.loss(), Power::from_picowatts(900_000_000_000));
    assert_eq!(
        transfer.output().checked_add(transfer.loss()),
        Some(transfer.input())
    );
}

#[test]
fn efficiency_handles_full_width_power_and_rounds_tiny_output_down() {
    let maximum =
        apply_mechanical_efficiency(Power::from_picowatts(u128::MAX), efficiency(999_999));
    assert_eq!(
        maximum.output().checked_add(maximum.loss()),
        Some(maximum.input())
    );

    let tiny = apply_mechanical_efficiency(Power::from_picowatts(1), efficiency(500_000));
    assert_eq!(tiny.output(), Power::ZERO);
    assert_eq!(tiny.loss(), Power::from_picowatts(1));
}

#[test]
fn efficiency_deserialization_rejects_values_above_unity() {
    let result: Result<MechanicalEfficiency, _> = serde_json::from_str("1000001");
    assert!(result.is_err());
}

fn ratio(numerator: u32, denominator: u32) -> TransmissionRatio {
    match TransmissionRatio::new(numerator, denominator) {
        Ok(ratio) => ratio,
        Err(error) => panic!("mechanical transmission ratio fixture failed: {error}"),
    }
}

#[test]
fn transmission_ratio_normalizes_equal_physical_ratios() {
    assert_eq!(ratio(6, 4), ratio(3, 2));
    assert_eq!(ratio(6, 4).numerator(), 3);
    assert_eq!(ratio(6, 4).denominator(), 2);
    assert_eq!(
        TransmissionRatio::new(0, 1),
        Err(TransmissionRatioError::ZeroNumerator)
    );
    assert_eq!(
        TransmissionRatio::new(1, 0),
        Err(TransmissionRatioError::ZeroDenominator)
    );
}

#[test]
fn transmission_ratio_deserialization_requires_canonical_terms() {
    let canonical: TransmissionRatio =
        match serde_json::from_str(r#"{"numerator":3,"denominator":2}"#) {
            Ok(ratio) => ratio,
            Err(error) => panic!("canonical transmission ratio failed decode: {error}"),
        };
    assert_eq!(canonical, ratio(3, 2));

    let noncanonical: Result<TransmissionRatio, _> =
        serde_json::from_str(r#"{"numerator":6,"denominator":4}"#);
    assert!(noncanonical.is_err());
}

#[test]
fn ideal_speed_increase_trades_torque_without_changing_power() {
    let input = RotationalOperatingPoint::new(
        Torque::from_micronewton_meters(6_000_000),
        AngularSpeed::from_microradians_per_second(1_000_000),
    );
    let transfer =
        match calculate_mechanical_transmission(input, ratio(3, 1), MechanicalEfficiency::IDEAL) {
            Ok(transfer) => transfer,
            Err(error) => panic!("ideal speed-up transmission failed: {error}"),
        };

    assert_eq!(
        transfer.output().angular_speed(),
        AngularSpeed::from_microradians_per_second(3_000_000)
    );
    assert_eq!(
        transfer.output().torque(),
        Torque::from_micronewton_meters(2_000_000)
    );
    assert_eq!(transfer.output().power(), input.power());
    assert_eq!(transfer.total_loss(), Power::ZERO);
}

#[test]
fn reduction_increases_torque_while_efficiency_becomes_explicit_loss() {
    let input = RotationalOperatingPoint::new(
        Torque::from_micronewton_meters(2_000_000),
        AngularSpeed::from_microradians_per_second(4_000_000),
    );
    let transfer = match calculate_mechanical_transmission(input, ratio(1, 4), efficiency(900_000))
    {
        Ok(transfer) => transfer,
        Err(error) => panic!("reduction transmission failed: {error}"),
    };

    assert_eq!(
        transfer.output().angular_speed(),
        AngularSpeed::from_microradians_per_second(1_000_000)
    );
    assert_eq!(
        transfer.output().torque(),
        Torque::from_micronewton_meters(7_200_000)
    );
    assert_eq!(
        transfer.output().power(),
        Power::from_picowatts(7_200_000_000_000)
    );
    assert_eq!(
        transfer.modeled_loss(),
        Power::from_picowatts(800_000_000_000)
    );
    assert_eq!(transfer.quantization_loss(), Power::ZERO);
    assert_eq!(
        transfer.output().power().checked_add(transfer.total_loss()),
        Some(input.power())
    );
}

#[test]
fn transmission_rounding_can_only_move_unrepresentable_power_into_loss() {
    let input = RotationalOperatingPoint::new(
        Torque::from_micronewton_meters(1),
        AngularSpeed::from_microradians_per_second(1),
    );
    let transfer =
        match calculate_mechanical_transmission(input, ratio(2, 3), MechanicalEfficiency::IDEAL) {
            Ok(transfer) => transfer,
            Err(error) => panic!("quantized transmission failed: {error}"),
        };

    assert_eq!(transfer.output().power(), Power::ZERO);
    assert_eq!(transfer.modeled_loss(), Power::ZERO);
    assert_eq!(transfer.quantization_loss(), Power::from_picowatts(1));
    assert_eq!(transfer.total_loss(), input.power());
}

#[test]
fn transmission_grid_never_creates_or_loses_accounted_power() {
    let torques = [0_u64, 1, 7, 1_000, 1_000_000];
    let speeds = [0_u64, 1, 11, 2_000, 3_000_000];
    let ratios = [
        ratio(1, 1),
        ratio(2, 1),
        ratio(1, 2),
        ratio(3, 2),
        ratio(2, 3),
    ];
    let efficiencies = [
        MechanicalEfficiency::ZERO,
        efficiency(1),
        efficiency(333_333),
        efficiency(999_999),
        MechanicalEfficiency::IDEAL,
    ];

    for torque in torques {
        for speed in speeds {
            let input = RotationalOperatingPoint::new(
                Torque::from_micronewton_meters(torque),
                AngularSpeed::from_microradians_per_second(speed),
            );
            for ratio in ratios {
                for efficiency in efficiencies {
                    let transfer = match calculate_mechanical_transmission(input, ratio, efficiency)
                    {
                        Ok(transfer) => transfer,
                        Err(error) => panic!(
                            "bounded transmission grid unexpectedly overflowed for torque {torque}, speed {speed}, ratio {}/{}, efficiency {} ppm: {error}",
                            ratio.numerator(),
                            ratio.denominator(),
                            efficiency.parts_per_million()
                        ),
                    };
                    let accounted =
                        match transfer.output().power().checked_add(transfer.total_loss()) {
                            Some(accounted) => accounted,
                            None => {
                                panic!("bounded transmission accounting overflowed input power")
                            }
                        };
                    assert_eq!(accounted, input.power());
                    assert!(transfer.output().power() <= input.power());
                }
            }
        }
    }
}

#[test]
fn extreme_ratios_reject_output_quantity_overflow() {
    let torque_heavy = RotationalOperatingPoint::new(
        Torque::from_micronewton_meters(u64::MAX),
        AngularSpeed::ZERO,
    );
    assert_eq!(
        calculate_mechanical_transmission(
            torque_heavy,
            ratio(1, u32::MAX),
            MechanicalEfficiency::IDEAL,
        ),
        Err(MechanicalTransmissionError::TorqueOutOfRange)
    );

    let speed_heavy = RotationalOperatingPoint::new(
        Torque::ZERO,
        AngularSpeed::from_microradians_per_second(u64::MAX),
    );
    assert_eq!(
        calculate_mechanical_transmission(
            speed_heavy,
            ratio(u32::MAX, 1),
            MechanicalEfficiency::IDEAL,
        ),
        Err(MechanicalTransmissionError::AngularSpeedOutOfRange)
    );
}
