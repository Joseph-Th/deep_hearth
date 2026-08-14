//! Finite fluid ownership and exact flow integration; sibling modules separate definitions, state, accounting, and canonical storage mutation.

mod accounting;
mod definitions;
mod state;
mod storage_execution;

pub use accounting::{
    FluidVolumeAccounting, FluidVolumeAccountingError, calculate_fluid_volume_accounting,
};
pub use definitions::{FluidDefinition, FluidDefinitionId, FluidRegistry};
pub use state::{FluidContents, FluidState, FluidStoreId, FluidStoreRecord, FluidValidationError};
pub use storage_execution::{
    AddFluidStoreError, FluidTransferCommitError, FluidTransferError, FluidTransferOutcome,
    FluidTransferResolution, ValidatedFluidTransfer, add_fluid_store, validate_fluid_transfer,
};

pub(crate) use state::validate_loaded_fluid;

use std::error::Error;
use std::fmt::{Display, Formatter};
use std::num::NonZeroU16;

use serde::{Deserialize, Serialize};

use crate::core::quantity::{Volume, VolumetricFlow};
use crate::core::time::TickSpan;

/// Fractional microliter numerator retained between flow-integration steps.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlowRemainder(u16);

impl FlowRemainder {
    pub const ZERO: Self = Self(0);

    #[must_use]
    pub const fn numerator(self) -> u16 {
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
    InvalidRemainder {
        remainder: FlowRemainder,
        ticks_per_second: NonZeroU16,
    },
    ArithmeticOverflow,
    VolumeOutOfRange,
}

impl Display for FlowIntegrationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRemainder {
                remainder,
                ticks_per_second,
            } => write!(
                formatter,
                "flow remainder {} is not below tick-rate denominator {}",
                remainder.numerator(),
                ticks_per_second.get()
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
/// would create systematic water/material loss at flow rates that do not divide evenly by tick rate.
pub fn integrate_flow(
    flow: VolumetricFlow,
    span: TickSpan,
    ticks_per_second: NonZeroU16,
    prior_remainder: FlowRemainder,
) -> Result<FlowIntegration, FlowIntegrationError> {
    if prior_remainder.numerator() >= ticks_per_second.get() {
        return Err(FlowIntegrationError::InvalidRemainder {
            remainder: prior_remainder,
            ticks_per_second,
        });
    }
    let numerator = u128::from(flow.microliters_per_second())
        .checked_mul(u128::from(span.value()))
        .and_then(|value| value.checked_add(u128::from(prior_remainder.numerator())))
        .ok_or(FlowIntegrationError::ArithmeticOverflow)?;
    let denominator = u128::from(ticks_per_second.get());
    let whole_volume = numerator / denominator;
    let remainder_value = numerator % denominator;
    let whole_volume =
        u64::try_from(whole_volume).map_err(|_| FlowIntegrationError::VolumeOutOfRange)?;
    let remainder = match u16::try_from(remainder_value) {
        Ok(value) => FlowRemainder(value),
        Err(_) => return Err(FlowIntegrationError::ArithmeticOverflow),
    };
    Ok(FlowIntegration {
        volume: Volume::from_microliters(whole_volume),
        remainder,
    })
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
    fn fractional_flow_is_conserved_across_repeated_ticks() {
        let rate = tick_rate(20);
        let mut remainder = FlowRemainder::ZERO;
        let mut volume = Volume::ZERO;

        for _ in 0..20 {
            let result = match integrate_flow(
                VolumetricFlow::from_microliters_per_second(1),
                TickSpan::new(1),
                rate,
                remainder,
            ) {
                Ok(result) => result,
                Err(error) => panic!("flow integration failed: {error}"),
            };
            volume = match volume.checked_add(result.volume()) {
                Some(value) => value,
                None => panic!("test volume accumulation overflowed"),
            };
            remainder = result.remainder();
        }

        assert_eq!(volume, Volume::from_microliters(1));
        assert_eq!(remainder, FlowRemainder::ZERO);
    }

    #[test]
    fn whole_second_flow_matches_authored_rate_exactly() {
        let result = match integrate_flow(
            VolumetricFlow::from_microliters_per_second(25_000),
            TickSpan::new(20),
            tick_rate(20),
            FlowRemainder::ZERO,
        ) {
            Ok(result) => result,
            Err(error) => panic!("flow integration failed: {error}"),
        };

        assert_eq!(result.volume(), Volume::from_microliters(25_000));
        assert_eq!(result.remainder(), FlowRemainder::ZERO);
    }
}
