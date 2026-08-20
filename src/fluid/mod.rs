//! Finite fluid ownership and exact flow integration; sibling modules separate definitions, state, accounting, and canonical storage mutation.

mod accounting;
mod definitions;
mod egress;
mod state;
mod storage_execution;
mod structural_integration;

pub use accounting::{
    FluidVolumeAccounting, FluidVolumeAccountingError, calculate_fluid_volume_accounting,
};
pub use definitions::{FluidDefinition, FluidDefinitionId, FluidRegistry};
pub use state::{FluidContents, FluidState, FluidStoreId, FluidStoreRecord, FluidValidationError};
pub use storage_execution::{
    FluidTransferCommitError, FluidTransferError, FluidTransferOutcome, FluidTransferResolution,
    ValidatedFluidTransfer, validate_fluid_transfer,
};
pub use structural_integration::{
    FluidStructuralLoadError, FluidSupportCommitError, FluidSupportError, FluidSupportOutcome,
    ValidatedFluidSupportChange, validate_mount_fluid_store, validate_unmount_fluid_store,
};

pub(crate) use egress::{
    FluidEgressCommitError, FluidEgressError, ValidatedFluidEgress, validate_fluid_egress,
};
pub(crate) use state::validate_loaded_fluid;
pub(crate) use structural_integration::validate_existing_fluid_load;

#[cfg(any(test, feature = "test-gameplay"))]
pub(crate) use storage_execution::add_fluid_store_with_contents_for_fixture;

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
mod tests {
    use super::*;

    const fn twentieth_second_tick() -> PhysicalTickDuration {
        PhysicalTickDuration::from_microseconds(50_000)
    }

    #[test]
    fn fractional_flow_is_conserved_across_repeated_ticks() {
        let tick_duration = twentieth_second_tick();
        let mut remainder = FlowRemainder::ZERO;
        let mut volume = Volume::ZERO;

        for _ in 0..20 {
            let result = match integrate_flow(
                VolumetricFlow::from_microliters_per_second(1),
                TickSpan::new(1),
                tick_duration,
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
            twentieth_second_tick(),
            FlowRemainder::ZERO,
        ) {
            Ok(result) => result,
            Err(error) => panic!("flow integration failed: {error}"),
        };

        assert_eq!(result.volume(), Volume::from_microliters(25_000));
        assert_eq!(result.remainder(), FlowRemainder::ZERO);
    }
}
