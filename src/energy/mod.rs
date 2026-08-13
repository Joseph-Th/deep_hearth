//! Exact power-to-energy integration across simulation ticks with explicit fractional remainder ownership.

use std::error::Error;
use std::fmt::{Display, Formatter};
use std::num::NonZeroU16;

use serde::{Deserialize, Serialize};

use crate::core::quantity::{Energy, Power};
use crate::core::time::TickSpan;

/// Fractional nanojoule numerator retained between power-integration steps.
///
/// Because power is stored in picowatts, the denominator is `ticks_per_second * 1000`. A future
/// energy owner that repeatedly integrates power must persist this remainder alongside its own
/// runtime state to avoid rounding loss.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PowerRemainder(u64);

impl PowerRemainder {
    pub const ZERO: Self = Self(0);

    #[must_use]
    pub const fn numerator(self) -> u64 {
        self.0
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
    InvalidRemainder {
        remainder: PowerRemainder,
        ticks_per_second: NonZeroU16,
    },
    ArithmeticOverflow,
}

impl Display for PowerIntegrationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRemainder {
                remainder,
                ticks_per_second,
            } => write!(
                formatter,
                "power remainder {} is not below integration denominator {}",
                remainder.numerator(),
                u32::from(ticks_per_second.get()) * 1_000
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
/// One thousand picowatts equal one nanojoule per second. The numerator is therefore
/// `power_pW * ticks + prior_remainder`; division by `ticks_per_second * 1000` yields whole
/// nanojoules and a remainder for the next call.
pub fn integrate_power(
    power: Power,
    span: TickSpan,
    ticks_per_second: NonZeroU16,
    prior_remainder: PowerRemainder,
) -> Result<PowerIntegration, PowerIntegrationError> {
    let denominator_u64 = u64::from(ticks_per_second.get()) * 1_000;
    if prior_remainder.numerator() >= denominator_u64 {
        return Err(PowerIntegrationError::InvalidRemainder {
            remainder: prior_remainder,
            ticks_per_second,
        });
    }
    let numerator = power
        .picowatts()
        .checked_mul(u128::from(span.value()))
        .and_then(|value| value.checked_add(u128::from(prior_remainder.numerator())))
        .ok_or(PowerIntegrationError::ArithmeticOverflow)?;
    let denominator = u128::from(denominator_u64);
    let energy = Energy::from_nanojoules(numerator / denominator);
    let remainder_value = numerator % denominator;
    let remainder = match u64::try_from(remainder_value) {
        Ok(value) => PowerRemainder(value),
        Err(_) => return Err(PowerIntegrationError::ArithmeticOverflow),
    };
    Ok(PowerIntegration { energy, remainder })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tick_rate(value: u16) -> NonZeroU16 {
        match NonZeroU16::new(value) {
            Some(value) => value,
            None => panic!("test tick rate must be nonzero"),
        }
    }

    #[test]
    fn twenty_hertz_power_integration_is_exact_for_one_microwatt() {
        let result = match integrate_power(
            Power::from_microwatts(1),
            TickSpan::new(1),
            tick_rate(20),
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
        let rate = tick_rate(60);
        let mut remainder = PowerRemainder::ZERO;
        let mut accumulated = Energy::ZERO;
        for _ in 0..60 {
            let result =
                match integrate_power(Power::from_microwatts(1), TickSpan::new(1), rate, remainder)
                {
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
}
