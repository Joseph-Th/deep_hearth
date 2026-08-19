//! Exact scalar rotational mechanics; topology, shafts, gearing, slip, inertia, and network ownership remain separate future systems.

use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Deserializer, Serialize};

use crate::core::quantity::{AngularSpeed, Power, Torque};

/// Normalization scale for authored mechanical transfer efficiency.
pub const MECHANICAL_EFFICIENCY_PARTS_PER_MILLION: u32 = 1_000_000;

/// Fraction of mechanical input power reaching the output after modeled scalar losses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub struct MechanicalEfficiency(u32);

impl MechanicalEfficiency {
    pub const ZERO: Self = Self(0);
    pub const IDEAL: Self = Self(MECHANICAL_EFFICIENCY_PARTS_PER_MILLION);

    pub fn new(parts_per_million: u32) -> Result<Self, MechanicalEfficiencyError> {
        if parts_per_million > MECHANICAL_EFFICIENCY_PARTS_PER_MILLION {
            return Err(MechanicalEfficiencyError::OutOfRange { parts_per_million });
        }
        Ok(Self(parts_per_million))
    }

    #[must_use]
    pub const fn parts_per_million(self) -> u32 {
        self.0
    }
}

impl<'de> Deserialize<'de> for MechanicalEfficiency {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = u32::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

/// Invalid normalized mechanical efficiency.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MechanicalEfficiencyError {
    OutOfRange { parts_per_million: u32 },
}

impl Display for MechanicalEfficiencyError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OutOfRange { parts_per_million } => write!(
                formatter,
                "mechanical efficiency {parts_per_million} ppm exceeds {MECHANICAL_EFFICIENCY_PARTS_PER_MILLION} ppm"
            ),
        }
    }
}

impl Error for MechanicalEfficiencyError {}

/// One exact rotational operating point and its derived mechanical power.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RotationalOperatingPoint {
    torque: Torque,
    angular_speed: AngularSpeed,
    power: Power,
}

impl RotationalOperatingPoint {
    /// Builds one physically defined rotational point without applying any provider envelope.
    pub fn new(torque: Torque, angular_speed: AngularSpeed) -> Self {
        Self {
            torque,
            angular_speed,
            power: calculate_rotational_power(torque, angular_speed),
        }
    }

    #[must_use]
    pub const fn torque(self) -> Torque {
        self.torque
    }

    #[must_use]
    pub const fn angular_speed(self) -> AngularSpeed {
        self.angular_speed
    }

    #[must_use]
    pub const fn power(self) -> Power {
        self.power
    }
}

/// Independent authored upper bounds for a rotational provider or machine interface.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RotationalLimits {
    max_torque: Torque,
    max_angular_speed: AngularSpeed,
    max_power: Power,
}

impl RotationalLimits {
    pub const fn new(
        max_torque: Torque,
        max_angular_speed: AngularSpeed,
        max_power: Power,
    ) -> Self {
        Self {
            max_torque,
            max_angular_speed,
            max_power,
        }
    }

    #[must_use]
    pub const fn max_torque(self) -> Torque {
        self.max_torque
    }

    #[must_use]
    pub const fn max_angular_speed(self) -> AngularSpeed {
        self.max_angular_speed
    }

    #[must_use]
    pub const fn max_power(self) -> Power {
        self.max_power
    }
}

/// Why a requested rotational point lies outside an authored physical envelope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RotationalOperatingPointError {
    TorqueExceeded {
        requested: Torque,
        maximum: Torque,
    },
    AngularSpeedExceeded {
        requested: AngularSpeed,
        maximum: AngularSpeed,
    },
    PowerExceeded {
        requested: Power,
        maximum: Power,
    },
}

impl Display for RotationalOperatingPointError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TorqueExceeded { requested, maximum } => write!(
                formatter,
                "requested torque {} uN*m exceeds maximum {} uN*m",
                requested.micronewton_meters(),
                maximum.micronewton_meters()
            ),
            Self::AngularSpeedExceeded { requested, maximum } => write!(
                formatter,
                "requested angular speed {} urad/s exceeds maximum {} urad/s",
                requested.microradians_per_second(),
                maximum.microradians_per_second()
            ),
            Self::PowerExceeded { requested, maximum } => write!(
                formatter,
                "requested rotational power {} pW exceeds maximum {} pW",
                requested.picowatts(),
                maximum.picowatts()
            ),
        }
    }
}

impl Error for RotationalOperatingPointError {}

/// Calculates `P = torque * angular_speed` exactly at the authoritative unit scales.
///
/// `1 micronewton-meter * 1 microradian/second = 1 picowatt`. Both source quantities use `u64`,
/// so their full-width product is always representable by the `u128` power backing type.
#[must_use]
pub fn calculate_rotational_power(torque: Torque, angular_speed: AngularSpeed) -> Power {
    Power::from_picowatts(
        u128::from(torque.micronewton_meters())
            * u128::from(angular_speed.microradians_per_second()),
    )
}

/// Validates one requested torque/speed point against independent torque, speed, and power limits.
pub fn validate_rotational_operating_point(
    limits: RotationalLimits,
    torque: Torque,
    angular_speed: AngularSpeed,
) -> Result<RotationalOperatingPoint, RotationalOperatingPointError> {
    if torque > limits.max_torque() {
        return Err(RotationalOperatingPointError::TorqueExceeded {
            requested: torque,
            maximum: limits.max_torque(),
        });
    }
    if angular_speed > limits.max_angular_speed() {
        return Err(RotationalOperatingPointError::AngularSpeedExceeded {
            requested: angular_speed,
            maximum: limits.max_angular_speed(),
        });
    }
    let point = RotationalOperatingPoint::new(torque, angular_speed);
    let power = point.power();
    if power > limits.max_power() {
        return Err(RotationalOperatingPointError::PowerExceeded {
            requested: power,
            maximum: limits.max_power(),
        });
    }
    Ok(point)
}

/// Explicit split between useful output power and modeled mechanical loss.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MechanicalPowerTransfer {
    input: Power,
    output: Power,
    loss: Power,
}

impl MechanicalPowerTransfer {
    #[must_use]
    pub const fn input(self) -> Power {
        self.input
    }

    #[must_use]
    pub const fn output(self) -> Power {
        self.output
    }

    #[must_use]
    pub const fn loss(self) -> Power {
        self.loss
    }
}

/// Applies an authored scalar efficiency without overflowing full-width power and without creating
/// power through rounding. Fractional picowatts are conservatively assigned to loss.
pub fn apply_mechanical_efficiency(
    input: Power,
    efficiency: MechanicalEfficiency,
) -> MechanicalPowerTransfer {
    let denominator = u128::from(MECHANICAL_EFFICIENCY_PARTS_PER_MILLION);
    let numerator = u128::from(efficiency.parts_per_million());
    let input_value = input.picowatts();
    let whole = (input_value / denominator) * numerator;
    let fractional = (input_value % denominator) * numerator / denominator;
    let output_value = whole + fractional;
    debug_assert!(output_value <= input_value);
    let output = Power::from_picowatts(output_value);
    let loss = Power::from_picowatts(input_value - output_value);
    MechanicalPowerTransfer {
        input,
        output,
        loss,
    }
}

/// Positive rational output-speed/input-speed ratio for an idealized rotational transmission.
///
/// A ratio of `3/1` triples angular speed and correspondingly divides ideal torque by three. A
/// ratio of `1/4` quarters speed and multiplies ideal torque by four. Construction normalizes the
/// fraction so equal physical ratios compare equal and serialize consistently when embedded later.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub struct TransmissionRatio {
    numerator: u32,
    denominator: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TransmissionRatioRepr {
    numerator: u32,
    denominator: u32,
}

impl<'de> Deserialize<'de> for TransmissionRatio {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let repr = TransmissionRatioRepr::deserialize(deserializer)?;
        let normalized =
            Self::new(repr.numerator, repr.denominator).map_err(serde::de::Error::custom)?;
        if normalized.numerator != repr.numerator || normalized.denominator != repr.denominator {
            return Err(serde::de::Error::custom(
                TransmissionRatioError::NonCanonical {
                    numerator: repr.numerator,
                    denominator: repr.denominator,
                },
            ));
        }
        Ok(normalized)
    }
}

impl TransmissionRatio {
    pub fn new(numerator: u32, denominator: u32) -> Result<Self, TransmissionRatioError> {
        if numerator == 0 {
            return Err(TransmissionRatioError::ZeroNumerator);
        }
        if denominator == 0 {
            return Err(TransmissionRatioError::ZeroDenominator);
        }
        let divisor = greatest_common_divisor(numerator, denominator);
        Ok(Self {
            numerator: numerator / divisor,
            denominator: denominator / divisor,
        })
    }

    #[must_use]
    pub const fn numerator(self) -> u32 {
        self.numerator
    }

    #[must_use]
    pub const fn denominator(self) -> u32 {
        self.denominator
    }
}

fn greatest_common_divisor(mut left: u32, mut right: u32) -> u32 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

/// Invalid authored rotational ratio.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransmissionRatioError {
    ZeroNumerator,
    ZeroDenominator,
    NonCanonical { numerator: u32, denominator: u32 },
}

impl Display for TransmissionRatioError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroNumerator => {
                formatter.write_str("mechanical transmission ratio numerator must be nonzero")
            }
            Self::ZeroDenominator => {
                formatter.write_str("mechanical transmission ratio denominator must be nonzero")
            }
            Self::NonCanonical {
                numerator,
                denominator,
            } => write!(
                formatter,
                "mechanical transmission ratio {numerator}/{denominator} is not reduced to canonical terms"
            ),
        }
    }
}

impl Error for TransmissionRatioError {}

/// Scalar result of one ratio and efficiency transformation before any network/topology solver.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MechanicalTransmission {
    input: RotationalOperatingPoint,
    output: RotationalOperatingPoint,
    ratio: TransmissionRatio,
    efficiency: MechanicalEfficiency,
    modeled_loss: Power,
    quantization_loss: Power,
}

impl MechanicalTransmission {
    pub const fn input(self) -> RotationalOperatingPoint {
        self.input
    }

    pub const fn output(self) -> RotationalOperatingPoint {
        self.output
    }

    pub const fn ratio(self) -> TransmissionRatio {
        self.ratio
    }

    #[must_use]
    pub const fn efficiency(self) -> MechanicalEfficiency {
        self.efficiency
    }

    /// Power removed by the authored physical efficiency before integer output resolution.
    #[must_use]
    pub const fn modeled_loss(self) -> Power {
        self.modeled_loss
    }

    /// Additional sub-unit power that cannot be represented at the output torque/speed scales.
    #[must_use]
    pub const fn quantization_loss(self) -> Power {
        self.quantization_loss
    }

    #[must_use]
    pub fn total_loss(self) -> Power {
        match self.modeled_loss.checked_add(self.quantization_loss) {
            Some(loss) => loss,
            None => panic!("mechanical transmission loss components exceeded input power"),
        }
    }
}

/// Arithmetic failure while transforming one rotational operating point through a scalar ratio.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MechanicalTransmissionError {
    AngularSpeedOutOfRange,
    TorqueOutOfRange,
}

impl Display for MechanicalTransmissionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AngularSpeedOutOfRange => {
                formatter.write_str("transmitted angular speed exceeds authoritative range")
            }
            Self::TorqueOutOfRange => {
                formatter.write_str("transmitted torque exceeds authoritative range")
            }
        }
    }
}

impl Error for MechanicalTransmissionError {}

/// Resolves a scalar torque/speed transformation with explicit loss and conservative rounding.
///
/// This is deliberately not a shaft-network solver. It provides the local physical operation a
/// later gear, belt, pulley, clutch, or gearbox edge can reuse. Output speed and torque are both
/// rounded down, so represented output power can never exceed efficiency-adjusted input power.
pub fn calculate_mechanical_transmission(
    input: RotationalOperatingPoint,
    ratio: TransmissionRatio,
    efficiency: MechanicalEfficiency,
) -> Result<MechanicalTransmission, MechanicalTransmissionError> {
    let output_speed_value = u128::from(input.angular_speed().microradians_per_second())
        * u128::from(ratio.numerator())
        / u128::from(ratio.denominator());
    let output_speed_value = u64::try_from(output_speed_value)
        .map_err(|_| MechanicalTransmissionError::AngularSpeedOutOfRange)?;

    let torque_numerator = u128::from(input.torque().micronewton_meters())
        * u128::from(ratio.denominator())
        * u128::from(efficiency.parts_per_million());
    let torque_denominator =
        u128::from(ratio.numerator()) * u128::from(MECHANICAL_EFFICIENCY_PARTS_PER_MILLION);
    let output_torque_value = torque_numerator / torque_denominator;
    let output_torque_value = u64::try_from(output_torque_value)
        .map_err(|_| MechanicalTransmissionError::TorqueOutOfRange)?;

    let output = RotationalOperatingPoint::new(
        Torque::from_micronewton_meters(output_torque_value),
        AngularSpeed::from_microradians_per_second(output_speed_value),
    );
    let efficient = apply_mechanical_efficiency(input.power(), efficiency);
    debug_assert!(output.power() <= efficient.output());
    let quantization_loss_value = efficient.output().picowatts() - output.power().picowatts();

    Ok(MechanicalTransmission {
        input,
        output,
        ratio,
        efficiency,
        modeled_loss: efficient.loss(),
        quantization_loss: Power::from_picowatts(quantization_loss_value),
    })
}

#[cfg(test)]
mod tests {
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
        let transfer = match calculate_mechanical_transmission(
            input,
            ratio(3, 1),
            MechanicalEfficiency::IDEAL,
        ) {
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
        let transfer =
            match calculate_mechanical_transmission(input, ratio(1, 4), efficiency(900_000)) {
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
        let transfer = match calculate_mechanical_transmission(
            input,
            ratio(2, 3),
            MechanicalEfficiency::IDEAL,
        ) {
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
                        let transfer = match calculate_mechanical_transmission(
                            input, ratio, efficiency,
                        ) {
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
}
