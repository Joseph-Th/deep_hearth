//! Shared condition-adjusted equipment limits and transfer timing for thermal operations.

use crate::capability::{CapabilityId, CapabilityValue};
use crate::core::quantity::{Energy, Mass, Power, Temperature};
use crate::core::time::TickSpan;
use crate::energy::{PowerDurationError, calculate_power_duration_ceiling};
use crate::equipment::{EquipmentDefinition, resolve_equipment_capability};
use crate::maintenance::{
    ActiveConditionDurationError, Condition, calculate_usable_condition_after_active_ticks,
};
use crate::registry::Registries;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ThermalPowerTemperatureError {
    MissingTransferPower,
    MissingMaximumTemperature,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ThermalPowerTemperatureLimits {
    transfer_power: Power,
    maximum_temperature: Temperature,
}

impl ThermalPowerTemperatureLimits {
    #[must_use]
    pub(super) const fn transfer_power(self) -> Power {
        self.transfer_power
    }

    #[must_use]
    pub(super) const fn maximum_temperature(self) -> Temperature {
        self.maximum_temperature
    }
}

/// Resolves the two condition-sensitive thermal capabilities shared by heating and phase change.
pub(super) fn resolve_thermal_power_temperature_limits(
    equipment: &EquipmentDefinition,
    condition: Condition,
    transfer_power_capability: CapabilityId,
    maximum_temperature_capability: CapabilityId,
) -> Result<ThermalPowerTemperatureLimits, ThermalPowerTemperatureError> {
    let transfer_power =
        match resolve_equipment_capability(equipment, condition, transfer_power_capability) {
            Some(CapabilityValue::Power(power)) => power,
            Some(_) | None => return Err(ThermalPowerTemperatureError::MissingTransferPower),
        };
    let maximum_temperature =
        match resolve_equipment_capability(equipment, condition, maximum_temperature_capability) {
            Some(CapabilityValue::Temperature(temperature)) => temperature,
            Some(_) | None => return Err(ThermalPowerTemperatureError::MissingMaximumTemperature),
        };
    Ok(ThermalPowerTemperatureLimits {
        transfer_power,
        maximum_temperature,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ThermalBatchLimitError {
    MissingMaximumBatchMass,
    BatchMassExceeded { selected: Mass, maximum: Mass },
}

/// Validates one selected thermal batch against condition-adjusted equipment capacity.
pub(super) fn validate_thermal_batch_mass(
    equipment: &EquipmentDefinition,
    condition: Condition,
    maximum_batch_mass_capability: CapabilityId,
    selected_mass: Mass,
) -> Result<(), ThermalBatchLimitError> {
    let maximum =
        match resolve_equipment_capability(equipment, condition, maximum_batch_mass_capability) {
            Some(CapabilityValue::Mass(mass)) => mass,
            Some(_) | None => return Err(ThermalBatchLimitError::MissingMaximumBatchMass),
        };
    if selected_mass > maximum {
        return Err(ThermalBatchLimitError::BatchMassExceeded {
            selected: selected_mass,
            maximum,
        });
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ThermalTransferTimingError {
    Duration(PowerDurationError),
    ConditionDuration(ActiveConditionDurationError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ThermalTransferTiming {
    transfer_power: Power,
    duration: TickSpan,
    condition_after: Condition,
}

impl ThermalTransferTiming {
    #[must_use]
    pub(super) const fn transfer_power(self) -> Power {
        self.transfer_power
    }

    #[must_use]
    pub(super) const fn duration(self) -> TickSpan {
        self.duration
    }

    #[must_use]
    pub(super) const fn condition_after(self) -> Condition {
        self.condition_after
    }
}

/// Resolves the actual transfer bottleneck, exact active duration, and resulting equipment wear.
pub(super) fn resolve_thermal_transfer_timing(
    registries: &Registries,
    equipment_transfer_power: Power,
    external_transfer_power: Power,
    energy: Energy,
    condition_wear_ppm_per_active_tick: u32,
    condition_before: Condition,
) -> Result<ThermalTransferTiming, ThermalTransferTimingError> {
    let transfer_power = equipment_transfer_power.min(external_transfer_power);
    let duration = calculate_power_duration_ceiling(
        transfer_power,
        energy,
        registries.core().physical_tick_duration(),
    )
    .map_err(ThermalTransferTimingError::Duration)?;
    let condition_after = calculate_usable_condition_after_active_ticks(
        condition_wear_ppm_per_active_tick,
        condition_before,
        duration,
    )
    .map_err(ThermalTransferTimingError::ConditionDuration)?;
    Ok(ThermalTransferTiming {
        transfer_power,
        duration,
        condition_after,
    })
}
