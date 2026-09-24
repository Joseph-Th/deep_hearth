//! Shared condition-adjusted equipment limits and transfer timing for thermal operations.

use crate::capability::{CapabilityEvaluationError, CapabilityId, CapabilityValue};
use crate::core::quantity::{Energy, Mass, Power, Temperature};
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::energy::{PowerDurationError, calculate_power_duration_ceiling};
use crate::equipment::{
    EquipmentDefinition, EquipmentId, EquipmentProviderError, ResolvedEquipmentProvider,
    ValidatedEquipmentUse, evaluate_equipment_capabilities_at_condition,
    resolve_equipment_capability, resolve_equipment_provider,
};
use crate::maintenance::{
    ActiveConditionDurationError, Condition, calculate_usable_condition_after_active_ticks,
};
use crate::production::ProcessId;
use crate::registry::Registries;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ThermalEquipmentSetupError {
    Equipment(EquipmentProviderError),
    Capability(CapabilityEvaluationError),
    MissingTransferPower { capability: CapabilityId },
    MissingMaximumTemperature { capability: CapabilityId },
    MissingMaximumBatchMass { capability: CapabilityId },
    BatchMassExceeded { selected: Mass, maximum: Mass },
}

/// One runtime equipment requirement set for a thermal production operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ThermalEquipmentRequest {
    process: ProcessId,
    equipment: EquipmentId,
    transfer_power_capability: CapabilityId,
    maximum_temperature_capability: CapabilityId,
    maximum_batch_mass_capability: CapabilityId,
    selected_mass: Mass,
}

impl ThermalEquipmentRequest {
    pub(super) const fn new(
        process: ProcessId,
        equipment: EquipmentId,
        transfer_power_capability: CapabilityId,
        maximum_temperature_capability: CapabilityId,
        maximum_batch_mass_capability: CapabilityId,
        selected_mass: Mass,
    ) -> Self {
        Self {
            process,
            equipment,
            transfer_power_capability,
            maximum_temperature_capability,
            maximum_batch_mass_capability,
            selected_mass,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ResolvedThermalEquipment<'state> {
    provider: ResolvedEquipmentProvider<'state>,
    equipment_use: ValidatedEquipmentUse,
    limits: ThermalPowerTemperatureLimits,
}

impl<'state> ResolvedThermalEquipment<'state> {
    pub(super) const fn provider(self) -> ResolvedEquipmentProvider<'state> {
        self.provider
    }

    pub(super) const fn equipment_use(self) -> ValidatedEquipmentUse {
        self.equipment_use
    }

    pub(super) const fn limits(self) -> ThermalPowerTemperatureLimits {
        self.limits
    }
}

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
            Some(
                CapabilityValue::Mass(_)
                | CapabilityValue::Temperature(_)
                | CapabilityValue::Pressure(_)
                | CapabilityValue::MassFlow(_),
            )
            | None => return Err(ThermalPowerTemperatureError::MissingTransferPower),
        };
    let maximum_temperature =
        match resolve_equipment_capability(equipment, condition, maximum_temperature_capability) {
            Some(CapabilityValue::Temperature(temperature)) => temperature,
            Some(
                CapabilityValue::Mass(_)
                | CapabilityValue::Power(_)
                | CapabilityValue::Pressure(_)
                | CapabilityValue::MassFlow(_),
            )
            | None => return Err(ThermalPowerTemperatureError::MissingMaximumTemperature),
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
        resolve_thermal_batch_mass_limit(equipment, condition, maximum_batch_mass_capability)?;
    if selected_mass > maximum {
        return Err(ThermalBatchLimitError::BatchMassExceeded {
            selected: selected_mass,
            maximum,
        });
    }
    Ok(())
}

/// Resolves the shared runtime equipment contract for one thermal production operation.
///
/// The thermal caller has already resolved its operation-specific definition. Registry validation
/// guarantees the matching production definition used by the shared capability evaluator below,
/// so this stage does not repeat public process-admission checks.
///
/// This deliberately uses the non-exclusive provider resolver. Production/mining/manual-power
/// occupancy remains visible to later process admission so resolution does not become
/// authorization; maintenance and prospecting still retain their direct-custody exclusion.
pub(super) fn resolve_runtime_thermal_equipment<'state>(
    registries: &'state Registries,
    state: &'state AppState,
    request: ThermalEquipmentRequest,
) -> Result<ResolvedThermalEquipment<'state>, ThermalEquipmentSetupError> {
    let provider = resolve_equipment_provider(registries, state, request.equipment)
        .map_err(ThermalEquipmentSetupError::Equipment)?;
    validate_thermal_process_capabilities(
        registries,
        request.process,
        provider.definition(),
        provider.condition(),
    )
    .map_err(ThermalEquipmentSetupError::Capability)?;
    let limits = resolve_thermal_power_temperature_limits(
        provider.definition(),
        provider.condition(),
        request.transfer_power_capability,
        request.maximum_temperature_capability,
    )
    .map_err(|error| match error {
        ThermalPowerTemperatureError::MissingTransferPower => {
            ThermalEquipmentSetupError::MissingTransferPower {
                capability: request.transfer_power_capability,
            }
        }
        ThermalPowerTemperatureError::MissingMaximumTemperature => {
            ThermalEquipmentSetupError::MissingMaximumTemperature {
                capability: request.maximum_temperature_capability,
            }
        }
    })?;
    validate_thermal_batch_mass(
        provider.definition(),
        provider.condition(),
        request.maximum_batch_mass_capability,
        request.selected_mass,
    )
    .map_err(|error| match error {
        ThermalBatchLimitError::MissingMaximumBatchMass => {
            ThermalEquipmentSetupError::MissingMaximumBatchMass {
                capability: request.maximum_batch_mass_capability,
            }
        }
        ThermalBatchLimitError::BatchMassExceeded { selected, maximum } => {
            ThermalEquipmentSetupError::BatchMassExceeded { selected, maximum }
        }
    })?;
    Ok(ResolvedThermalEquipment {
        equipment_use: provider.validated_use(),
        provider,
        limits,
    })
}

/// Evaluates every authored process capability against one equipment definition at one condition.
///
/// Runtime resolution, planning, and trusted-load replay share this exact derivation so generic
/// process requirements cannot drift away from thermal resolver-owned capability checks.
pub(super) fn validate_thermal_process_capabilities(
    registries: &Registries,
    process: ProcessId,
    equipment: &EquipmentDefinition,
    condition: Condition,
) -> Result<(), CapabilityEvaluationError> {
    let process_definition = registries
        .production()
        .get_process(process)
        .unwrap_or_else(|| {
            panic!(
                "validated thermal process {} lost its production definition",
                process.value()
            )
        });
    evaluate_equipment_capabilities_at_condition(
        registries.capabilities(),
        equipment,
        condition,
        process_definition.capability_requirements(),
    )
}

/// Resolves the condition-adjusted single-batch mass ceiling without selecting a batch amount.
pub(super) fn resolve_thermal_batch_mass_limit(
    equipment: &EquipmentDefinition,
    condition: Condition,
    maximum_batch_mass_capability: CapabilityId,
) -> Result<Mass, ThermalBatchLimitError> {
    match resolve_equipment_capability(equipment, condition, maximum_batch_mass_capability) {
        Some(CapabilityValue::Mass(mass)) => Ok(mass),
        Some(_) | None => Err(ThermalBatchLimitError::MissingMaximumBatchMass),
    }
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
