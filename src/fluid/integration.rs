//! Exact volumetric-flow integration with persisted fractional-volume remainder support.

use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::core::arithmetic::checked_mul_div_with_remainder;
use crate::core::quantity::{Volume, VolumetricFlow};
use crate::core::time::{PhysicalTickDuration, TickSpan};

const MICROLITER_MICROSECONDS_PER_MICROLITER_SECOND: u128 = 1_000_000;

/// Fractional microliter numerator retained between flow-integration steps.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlowRemainder(u32);

impl FlowRemainder {
    pub const ZERO: Self = Self(0);

    #[must_use]
    pub const fn numerator(self) -> u32 {
        self.0
    }
}

/// Whole-volume integration outcome plus carried fractional remainder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlowIntegration {
    volume: Volume,
    remainder: FlowRemainder,
}

impl FlowIntegration {
    #[must_use]
    pub const fn volume(self) -> Volume {
        self.volume
    }

    #[must_use]
    pub const fn remainder(self) -> FlowRemainder {
        self.remainder
    }
}

/// Invalid flow-integration state or arithmetic overflow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlowIntegrationError {
    InvalidRemainder { remainder: FlowRemainder },
    ArithmeticOverflow,
    VolumeOutOfRange,
}

impl Display for FlowIntegrationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRemainder { remainder } => write!(
                formatter,
                "flow remainder {} is not below integration denominator {}",
                remainder.numerator(),
                MICROLITER_MICROSECONDS_PER_MICROLITER_SECOND
            ),
            Self::ArithmeticOverflow => {
                formatter.write_str("flow integration overflowed intermediate storage")
            }
            Self::VolumeOutOfRange => {
                formatter.write_str("integrated flow exceeds authoritative volume range")
            }
        }
    }
}

impl Error for FlowIntegrationError {}

/// Integrates a constant microliter/second flow across an integer tick span.
///
/// A future fluid owner that calls this incrementally must persist `FlowRemainder`; discarding it
/// would create systematic water/material loss at rates that do not divide evenly across the
/// physical duration of a world tick.
pub fn integrate_flow(
    flow: VolumetricFlow,
    span: TickSpan,
    physical_tick_duration: PhysicalTickDuration,
    prior_remainder: FlowRemainder,
) -> Result<FlowIntegration, FlowIntegrationError> {
    if u128::from(prior_remainder.numerator()) >= MICROLITER_MICROSECONDS_PER_MICROLITER_SECOND {
        return Err(FlowIntegrationError::InvalidRemainder {
            remainder: prior_remainder,
        });
    }
    let elapsed_microseconds = physical_tick_duration.span_microseconds(span);
    let (whole_volume, remainder_value) = checked_mul_div_with_remainder(
        u128::from(flow.microliters_per_second()),
        elapsed_microseconds,
        MICROLITER_MICROSECONDS_PER_MICROLITER_SECOND,
        u128::from(prior_remainder.numerator()),
    )
    .ok_or(FlowIntegrationError::ArithmeticOverflow)?;
    let whole_volume =
        u64::try_from(whole_volume).map_err(|_| FlowIntegrationError::VolumeOutOfRange)?;
    let remainder = match u32::try_from(remainder_value) {
        Ok(value) => FlowRemainder(value),
        Err(_) => return Err(FlowIntegrationError::ArithmeticOverflow),
    };
    Ok(FlowIntegration {
        volume: Volume::from_microliters(whole_volume),
        remainder,
    })
}

#[cfg(test)]
#[path = "integration_tests.rs"]
mod tests;
