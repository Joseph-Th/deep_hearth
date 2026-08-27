//! Shared condition-adjusted physics for finite-energy ore-processing batches.

use crate::capability::{CapabilityId, CapabilityValue};
use crate::core::quantity::{Energy, Mass, MassFlow, Power};
use crate::core::time::TickSpan;
use crate::energy::{PowerDurationError, calculate_power_duration_ceiling};
use crate::equipment::{EquipmentDefinition, resolve_equipment_capability};
use crate::maintenance::{
    ActiveConditionDurationError, Condition, calculate_usable_condition_after_active_ticks,
};
use crate::registry::Registries;

use super::{MassFlowDurationError, calculate_mass_flow_duration_ceiling};

/// Failure while resolving condition-adjusted equipment limits for one powered ore batch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PoweredOreEquipmentError {
    MissingMassFlowCapability,
    MissingMaximumBatchMassCapability,
    BatchMassExceeded { selected: Mass, maximum: Mass },
}

/// Failure while resolving common active-time and wear physics after equipment admission.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PoweredOreTimingError {
    Throughput(MassFlowDurationError),
    Energy(PowerDurationError),
    Condition(ActiveConditionDurationError),
}

/// Condition-adjusted equipment throughput after common capability and batch-limit validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PoweredOreEquipment {
    processing_rate: MassFlow,
}

impl PoweredOreEquipment {
    #[must_use]
    pub(super) const fn processing_rate(self) -> MassFlow {
        self.processing_rate
    }
}

/// Exact active-time and wear result shared by admission and persistence replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PoweredOreTiming {
    throughput_duration: TickSpan,
    energy_duration: TickSpan,
    condition_after: Condition,
}

impl PoweredOreTiming {
    #[must_use]
    pub(super) const fn throughput_duration(self) -> TickSpan {
        self.throughput_duration
    }

    #[must_use]
    pub(super) const fn energy_duration(self) -> TickSpan {
        self.energy_duration
    }

    #[must_use]
    pub(super) fn duration(self) -> TickSpan {
        std::cmp::max(self.throughput_duration, self.energy_duration)
    }

    #[must_use]
    pub(super) const fn condition_after(self) -> Condition {
        self.condition_after
    }
}

/// Resolves common condition-adjusted equipment policy before output or energy admission.
///
/// Keeping this stage separate preserves the canonical error ordering: an impossible equipment
/// batch is rejected before process-specific output work or finite-energy validation is attempted.
pub(super) fn resolve_powered_ore_equipment(
    equipment: &EquipmentDefinition,
    condition_before: Condition,
    mass_flow_capability: CapabilityId,
    maximum_batch_mass_capability: CapabilityId,
    selected_mass: Mass,
) -> Result<PoweredOreEquipment, PoweredOreEquipmentError> {
    let processing_rate =
        match resolve_equipment_capability(equipment, condition_before, mass_flow_capability) {
            Some(CapabilityValue::MassFlow(rate)) => rate,
            Some(_) | None => return Err(PoweredOreEquipmentError::MissingMassFlowCapability),
        };
    let maximum_batch_mass = match resolve_equipment_capability(
        equipment,
        condition_before,
        maximum_batch_mass_capability,
    ) {
        Some(CapabilityValue::Mass(mass)) => mass,
        Some(_) | None => {
            return Err(PoweredOreEquipmentError::MissingMaximumBatchMassCapability);
        }
    };
    if selected_mass > maximum_batch_mass {
        return Err(PoweredOreEquipmentError::BatchMassExceeded {
            selected: selected_mass,
            maximum: maximum_batch_mass,
        });
    }

    Ok(PoweredOreEquipment { processing_rate })
}

/// Resolves common rate-bottleneck timing and condition wear after finite energy is validated.
pub(super) fn resolve_powered_ore_timing(
    registries: &Registries,
    processing_rate: MassFlow,
    selected_mass: Mass,
    required_energy: Energy,
    available_power: Power,
    condition_wear_ppm_per_active_tick: u32,
    condition_before: Condition,
) -> Result<PoweredOreTiming, PoweredOreTimingError> {
    let throughput_duration = calculate_mass_flow_duration_ceiling(
        processing_rate,
        selected_mass,
        registries.core().physical_tick_duration(),
    )
    .map_err(PoweredOreTimingError::Throughput)?;
    let energy_duration = calculate_power_duration_ceiling(
        available_power,
        required_energy,
        registries.core().physical_tick_duration(),
    )
    .map_err(PoweredOreTimingError::Energy)?;
    let duration = std::cmp::max(throughput_duration, energy_duration);
    let condition_after = calculate_usable_condition_after_active_ticks(
        condition_wear_ppm_per_active_tick,
        condition_before,
        duration,
    )
    .map_err(PoweredOreTimingError::Condition)?;

    Ok(PoweredOreTiming {
        throughput_duration,
        energy_duration,
        condition_after,
    })
}
