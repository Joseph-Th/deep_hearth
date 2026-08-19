//! Exact power-to-energy integration across simulation ticks with explicit fractional remainder ownership.

mod accounting;
mod construction_execution;
mod definitions;
mod disassembly_execution;
mod state;
mod storage_execution;
mod transfer_execution;

pub use accounting::{
    ExplicitEnergyAccounting, ExplicitEnergyAccountingError, calculate_explicit_energy_accounting,
};
pub use construction_execution::{
    EnergyStoreAssemblyCommitError, EnergyStoreAssemblyError, ValidatedEnergyStoreAssembly,
    validate_assemble_energy_store,
};

pub use definitions::{
    EnergyCarrier, EnergyRegistry, EnergyStoreDefinition, EnergyStoreDefinitionId,
};
pub use disassembly_execution::{
    EnergyStoreDisassemblyCommitError, EnergyStoreDisassemblyError, EnergyStoreDisassemblyOutcome,
    ValidatedEnergyStoreDisassembly, validate_disassemble_energy_store,
};
pub use state::{EnergyState, EnergyStoreId, EnergyStoreRecord, EnergyValidationError};
pub use storage_execution::{
    AddEnergyStoreError, ConsumedEnergyTrace, EnergySinkError, EnergySupplyError,
    ReleasedEnergyTrace, ValidatedEnergySink, ValidatedEnergySupply, add_energy_store,
    validate_energy_sink, validate_energy_supply,
};
pub use transfer_execution::{
    EnergyTransferCommitError, EnergyTransferError, EnergyTransferOutcome,
    EnergyTransferResolution, ValidatedEnergyTransfer, validate_energy_transfer,
};

#[cfg(feature = "test-gameplay")]
pub(crate) use storage_execution::add_energy_store_with_initial_for_fixture;
#[cfg(test)]
pub(crate) use storage_execution::add_energy_store_with_initial_for_fixture as add_energy_store_with_initial_for_test;
#[cfg(test)]
pub(crate) use transfer_execution::make_test_energy_transfer_resolution;

pub(crate) use state::validate_loaded_energy;
pub(crate) use storage_execution::{
    EnergyCommitError, EnergyConsumptionReservation, EnergyIngressReservation,
    EnergyIngressReservationError, EnergyReservationError, apply_energy_consumption_reservation,
    apply_released_energy_outcomes, validate_energy_consumption_reservation,
    validate_energy_ingress_reservation,
};

use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::core::arithmetic::checked_mul_div_with_remainder;
use crate::core::quantity::{Energy, Mass, MassSpecificEnergy, Power};
use crate::core::time::{PhysicalTickDuration, TickSpan};

const PICOWATT_MICROSECONDS_PER_NANOJOULE: u128 = 1_000_000_000;

/// Fractional nanojoule numerator retained between power-integration steps.
///
/// Because power is stored in picowatts and elapsed world-time in microseconds, one nanojoule is
/// exactly one billion picowatt-microseconds. A future energy owner that repeatedly integrates
/// power must persist this remainder alongside its own runtime state to avoid rounding loss.
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
mod tests {
    use super::*;

    const fn twentieth_second_tick() -> PhysicalTickDuration {
        PhysicalTickDuration::from_microseconds(50_000)
    }

    #[test]
    fn mass_specific_energy_scales_exactly_without_rounding() {
        assert_eq!(
            calculate_mass_specific_energy(
                Mass::from_milligrams(25),
                MassSpecificEnergy::from_nanojoules_per_milligram(40),
            ),
            Energy::from_nanojoules(1_000)
        );
    }

    #[test]
    fn twentieth_second_power_integration_is_exact_for_one_microwatt() {
        let result = match integrate_power(
            Power::from_microwatts(1),
            TickSpan::new(1),
            twentieth_second_tick(),
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
        let tick_duration = PhysicalTickDuration::from_microseconds(100_000);
        let mut remainder = PowerRemainder::ZERO;
        let mut accumulated = Energy::ZERO;
        for _ in 0..10 {
            let result = match integrate_power(
                Power::from_microwatts(1),
                TickSpan::new(1),
                tick_duration,
                remainder,
            ) {
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

    #[test]
    fn duration_ceiling_returns_first_tick_that_meets_energy_requirement() {
        let tick_duration = twentieth_second_tick();
        let required = Energy::from_nanojoules(51);
        let duration = match calculate_power_duration_ceiling(
            Power::from_microwatts(1),
            required,
            tick_duration,
        ) {
            Ok(duration) => duration,
            Err(error) => panic!("duration calculation failed: {error}"),
        };

        assert_eq!(duration, TickSpan::new(2));
        let one_tick = match integrate_power(
            Power::from_microwatts(1),
            TickSpan::new(1),
            tick_duration,
            PowerRemainder::ZERO,
        ) {
            Ok(result) => result.energy(),
            Err(error) => panic!("one-tick integration failed: {error}"),
        };
        let two_ticks = match integrate_power(
            Power::from_microwatts(1),
            duration,
            tick_duration,
            PowerRemainder::ZERO,
        ) {
            Ok(result) => result.energy(),
            Err(error) => panic!("two-tick integration failed: {error}"),
        };
        assert!(one_tick < required);
        assert!(two_ticks >= required);
    }

    #[test]
    fn duration_ceiling_rejects_nonzero_energy_at_zero_power() {
        assert_eq!(
            calculate_power_duration_ceiling(
                Power::ZERO,
                Energy::from_nanojoules(1),
                twentieth_second_tick(),
            ),
            Err(PowerDurationError::ZeroPower)
        );
    }

    #[test]
    fn duration_ceiling_handles_maximum_authoritative_values_without_overflow() {
        let duration = match calculate_power_duration_ceiling(
            Power::from_picowatts(u128::MAX),
            Energy::from_nanojoules(u128::MAX),
            PhysicalTickDuration::from_microseconds(1_000_000),
        ) {
            Ok(duration) => duration,
            Err(error) => panic!("maximum-value duration calculation failed: {error}"),
        };

        assert_eq!(duration, TickSpan::new(1_000));
    }

    #[test]
    fn duration_ceiling_reports_when_u64_tick_range_is_insufficient() {
        assert_eq!(
            calculate_power_duration_ceiling(
                Power::from_picowatts(1),
                Energy::from_nanojoules(u128::MAX),
                PhysicalTickDuration::from_microseconds(1),
            ),
            Err(PowerDurationError::DurationOverflow)
        );
    }
}
