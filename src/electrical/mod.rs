//! Exact electrical scalar calculations for potential, current, resistance, and power.

use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Deserializer, Serialize};

use crate::core::quantity::{ElectricCurrent, ElectricPotential, ElectricalResistance, Power};

/// Calculates electrical power exactly from microvolts and microamperes.
///
/// `1 µV * 1 µA = 1 pW`, matching the authoritative `Power` storage scale exactly.
#[must_use]
pub fn calculate_electrical_power(potential: ElectricPotential, current: ElectricCurrent) -> Power {
    Power::from_picowatts(u128::from(potential.microvolts()) * u128::from(current.microamperes()))
}

/// Sub-microvolt picovolt remainder from resistive voltage-drop calculation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct PotentialRemainderPicovolts(u32);

impl<'de> Deserialize<'de> for PotentialRemainderPicovolts {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = u32::deserialize(deserializer)?;
        let remainder = Self(value);
        remainder.validate().map_err(serde::de::Error::custom)?;
        Ok(remainder)
    }
}

impl PotentialRemainderPicovolts {
    pub const ZERO: Self = Self(0);

    #[must_use]
    pub const fn picovolts(self) -> u32 {
        self.0
    }

    pub fn validate(self) -> Result<(), ElectricalCalculationError> {
        if self.0 >= 1_000_000 {
            return Err(ElectricalCalculationError::InvalidPotentialRemainder {
                remainder_picovolts: self.0,
            });
        }
        Ok(())
    }
}

/// Whole-microvolt voltage drop plus retained sub-microvolt remainder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VoltageDrop {
    potential: ElectricPotential,
    remainder: PotentialRemainderPicovolts,
}

impl VoltageDrop {
    #[must_use]
    pub const fn potential(self) -> ElectricPotential {
        self.potential
    }

    #[must_use]
    pub const fn remainder(self) -> PotentialRemainderPicovolts {
        self.remainder
    }
}

/// Arithmetic failure in a scalar electrical calculation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ElectricalCalculationError {
    VoltageOutOfRange,
    InvalidPotentialRemainder { remainder_picovolts: u32 },
}

impl Display for ElectricalCalculationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::VoltageOutOfRange => {
                formatter.write_str("resistive voltage drop exceeds electric-potential range")
            }
            Self::InvalidPotentialRemainder {
                remainder_picovolts,
            } => write!(
                formatter,
                "potential remainder {remainder_picovolts} pV is not below one microvolt"
            ),
        }
    }
}

impl Error for ElectricalCalculationError {}

/// Calculates `V = I * R` from microamperes and microohms.
///
/// The raw product is in picovolts. Whole microvolts are returned as `ElectricPotential`; the
/// sub-microvolt picovolt remainder is explicit so a future network solver can preserve it instead
/// of repeatedly truncating small line drops.
pub fn calculate_resistive_voltage_drop(
    current: ElectricCurrent,
    resistance: ElectricalResistance,
) -> Result<VoltageDrop, ElectricalCalculationError> {
    let picovolts = u128::from(current.microamperes()) * u128::from(resistance.microohms());
    let whole_microvolts = picovolts / 1_000_000;
    let remainder_picovolts = picovolts % 1_000_000;
    let whole_microvolts = u64::try_from(whole_microvolts)
        .map_err(|_| ElectricalCalculationError::VoltageOutOfRange)?;
    let remainder_picovolts = match u32::try_from(remainder_picovolts) {
        Ok(value) => value,
        Err(_) => return Err(ElectricalCalculationError::VoltageOutOfRange),
    };
    Ok(VoltageDrop {
        potential: ElectricPotential::from_microvolts(whole_microvolts),
        remainder: PotentialRemainderPicovolts(remainder_picovolts),
    })
}

#[cfg(test)]
mod tests {
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
}
