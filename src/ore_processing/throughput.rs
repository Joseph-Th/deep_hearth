//! Shared scalar throughput-to-duration physics for ore and material preparation.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Mass, MassFlow};
use crate::core::time::{PhysicalTickDuration, TickSpan};

/// Failure to convert material throughput into a whole authoritative tick span.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MassFlowDurationError {
    ZeroRate,
    TickRangeExceeded,
}

impl Display for MassFlowDurationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroRate => formatter.write_str("material processing rate must be nonzero"),
            Self::TickRangeExceeded => {
                formatter.write_str("material processing duration exceeds authoritative tick range")
            }
        }
    }
}

impl Error for MassFlowDurationError {}

/// Returns the minimum whole tick span required to process an exact mass at a constant mass flow.
pub fn calculate_mass_flow_duration_ceiling(
    rate: MassFlow,
    mass: Mass,
    physical_tick_duration: PhysicalTickDuration,
) -> Result<TickSpan, MassFlowDurationError> {
    if rate.is_zero() {
        return Err(MassFlowDurationError::ZeroRate);
    }
    if mass.is_zero() {
        return Ok(TickSpan::ZERO);
    }
    let numerator = u128::from(mass.milligrams()) * 1_000_000;
    let denominator = u128::from(rate.milligrams_per_second())
        * u128::from(physical_tick_duration.microseconds());
    let ticks = numerator.div_ceil(denominator);
    let ticks = u64::try_from(ticks).map_err(|_| MassFlowDurationError::TickRangeExceeded)?;
    Ok(TickSpan::new(ticks))
}

/// Returns the greatest representable whole-milligram mass processable within an exact tick span.
///
/// This is the monotonic inverse of [`calculate_mass_flow_duration_ceiling`] for planning bounds.
/// When the physical capacity exceeds the `u64`-backed [`Mass`] range, the result is the largest
/// representable mass because no authoritative single-batch request can exceed that value.
#[must_use]
pub fn calculate_mass_flow_capacity(
    rate: MassFlow,
    duration: TickSpan,
    physical_tick_duration: PhysicalTickDuration,
) -> Mass {
    if rate.is_zero() || duration.is_zero() {
        return Mass::ZERO;
    }
    let capacity_numerator = u128::from(rate.milligrams_per_second())
        .checked_mul(u128::from(physical_tick_duration.microseconds()))
        .and_then(|value| value.checked_mul(u128::from(duration.value())));
    let Some(capacity_numerator) = capacity_numerator else {
        return Mass::from_milligrams(u64::MAX);
    };
    let milligrams = capacity_numerator / 1_000_000;
    Mass::from_milligrams(u64::try_from(milligrams).unwrap_or(u64::MAX))
}
