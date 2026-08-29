//! Owns exact power integration, duration inversion, and mass-specific energy conversion.

use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Deserializer, Serialize};

use crate::core::arithmetic::checked_mul_div_with_remainder;
use crate::core::quantity::{Energy, Mass, MassSpecificEnergy, Power};
use crate::core::time::{PhysicalTickDuration, TickSpan};

const PICOWATT_MICROSECONDS_PER_NANOJOULE: u128 = 1_000_000_000;

/// Fractional nanojoule numerator retained between power-integration steps.
///
/// Because power is stored in picowatts and elapsed world-time in microseconds, one nanojoule is
/// exactly one billion picowatt-microseconds. Repeated integration must persist this remainder in
/// the owning runtime state to avoid rounding loss.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct PowerRemainder(u64);

impl PowerRemainder {
    pub const ZERO: Self = Self(0);

    #[must_use]
    pub const fn numerator(self) -> u64 {
        self.0
    }
}

impl<'de> Deserialize<'de> for PowerRemainder {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let numerator = u64::deserialize(deserializer)?;
        if u128::from(numerator) >= PICOWATT_MICROSECONDS_PER_NANOJOULE {
            return Err(serde::de::Error::custom(format_args!(
                "power remainder {numerator} is not below integration denominator {PICOWATT_MICROSECONDS_PER_NANOJOULE}"
            )));
        }
        Ok(Self(numerator))
    }
}

/// Exact whole-nanojoule integration outcome plus the carried fractional remainder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PowerIntegration {
    energy: Energy,
    remainder: PowerRemainder,
}

impl PowerIntegration {
    #[must_use]
    pub const fn energy(self) -> Energy {
        self.energy
    }

    #[must_use]
    pub const fn remainder(self) -> PowerRemainder {
        self.remainder
    }
}

/// Invalid power-integration state or arithmetic overflow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerIntegrationError {
    InvalidRemainder { remainder: PowerRemainder },
    ArithmeticOverflow,
}

impl Display for PowerIntegrationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRemainder { remainder } => write!(
                formatter,
                "power remainder {} is not below integration denominator {}",
                remainder.numerator(),
                PICOWATT_MICROSECONDS_PER_NANOJOULE
            ),
            Self::ArithmeticOverflow => {
                formatter.write_str("power integration overflowed authoritative energy")
            }
        }
    }
}

impl Error for PowerIntegrationError {}

/// Integrates constant power over an integer tick span without discarding fractional nanojoules.
///
/// The numerator is `power_pW * physical_microseconds + prior_remainder`; division by one billion
/// yields whole nanojoules and a remainder for the next call.
pub fn integrate_power(
    power: Power,
    span: TickSpan,
    physical_tick_duration: PhysicalTickDuration,
    prior_remainder: PowerRemainder,
) -> Result<PowerIntegration, PowerIntegrationError> {
    if u128::from(prior_remainder.numerator()) >= PICOWATT_MICROSECONDS_PER_NANOJOULE {
        return Err(PowerIntegrationError::InvalidRemainder {
            remainder: prior_remainder,
        });
    }
    let elapsed_microseconds = physical_tick_duration.span_microseconds(span);
    let (energy_nanojoules, remainder_value) = checked_mul_div_with_remainder(
        power.picowatts(),
        elapsed_microseconds,
        PICOWATT_MICROSECONDS_PER_NANOJOULE,
        u128::from(prior_remainder.numerator()),
    )
    .ok_or(PowerIntegrationError::ArithmeticOverflow)?;
    let energy = Energy::from_nanojoules(energy_nanojoules);
    let remainder = match u64::try_from(remainder_value) {
        Ok(value) => PowerRemainder(value),
        Err(_) => return Err(PowerIntegrationError::ArithmeticOverflow),
    };
    Ok(PowerIntegration { energy, remainder })
}

/// Failure while converting an exact energy requirement into the minimum whole simulation ticks
/// at a constant power rate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerDurationError {
    ZeroPower,
    DurationOverflow,
}

impl Display for PowerDurationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroPower => {
                formatter.write_str("nonzero energy cannot be supplied at zero power")
            }
            Self::DurationOverflow => formatter.write_str(
                "required energy cannot be supplied within the authoritative tick-span range",
            ),
        }
    }
}

impl Error for PowerDurationError {}

fn has_integrated_energy_at_least(
    power: Power,
    ticks: u64,
    physical_tick_duration: PhysicalTickDuration,
    required: Energy,
) -> bool {
    match integrate_power(
        power,
        TickSpan::new(ticks),
        physical_tick_duration,
        PowerRemainder::ZERO,
    ) {
        Ok(integration) => integration.energy() >= required,
        Err(PowerIntegrationError::ArithmeticOverflow) => true,
        Err(PowerIntegrationError::InvalidRemainder { remainder: _ }) => {
            unreachable!("zero power remainder is always valid")
        }
    }
}

/// Returns the least whole tick span whose integrated constant power supplies at least `required`.
pub fn calculate_power_duration_ceiling(
    power: Power,
    required: Energy,
    physical_tick_duration: PhysicalTickDuration,
) -> Result<TickSpan, PowerDurationError> {
    if required.is_zero() {
        return Ok(TickSpan::ZERO);
    }
    if power.is_zero() {
        return Err(PowerDurationError::ZeroPower);
    }
    if !has_integrated_energy_at_least(power, u64::MAX, physical_tick_duration, required) {
        return Err(PowerDurationError::DurationOverflow);
    }

    let mut low = 1_u64;
    let mut high = u64::MAX;
    while low < high {
        let midpoint = low + (high - low) / 2;
        if has_integrated_energy_at_least(power, midpoint, physical_tick_duration, required) {
            high = midpoint;
        } else {
            low = midpoint + 1;
        }
    }
    Ok(TickSpan::new(low))
}

/// Resolves exact work/heat energy from material mass and an authored mass-specific requirement.
///
/// `Mass` and `MassSpecificEnergy` both use integer base units, and their `u64 * u64` product fits
/// exactly in the authoritative `u128` nanojoule representation.
#[must_use]
pub fn calculate_mass_specific_energy(mass: Mass, specific: MassSpecificEnergy) -> Energy {
    Energy::from_nanojoules(
        u128::from(mass.milligrams()) * u128::from(specific.nanojoules_per_milligram()),
    )
}

#[cfg(test)]
#[path = "integration_tests.rs"]
mod tests;
