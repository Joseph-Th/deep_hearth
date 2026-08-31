//! Shared condition-adjusted physics for finite-energy ore-processing batches.

use crate::capability::{CapabilityId, CapabilityValue};
use crate::core::quantity::{Energy, Mass, MassFlow, Power};
use crate::core::time::TickSpan;
use crate::energy::{
    EnergyCarrier, calculate_mass_specific_energy, calculate_power_duration_ceiling,
};
use crate::equipment::{EquipmentDefinition, resolve_equipment_capability};
use crate::maintenance::{Condition, calculate_usable_condition_after_active_ticks};
use crate::production::ProductionJobRecord;
use crate::registry::Registries;

use super::{PoweredOreProcessProfile, calculate_mass_flow_duration_ceiling};

mod errors;

pub use errors::PoweredOreJobValidationError;
pub(super) use errors::{PoweredOreEquipmentError, PoweredOreTimingError};

/// Physical rate constraint that determines one powered ore-processing duration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PoweredOreBottleneck {
    Throughput,
    EnergyDelivery,
    Balanced,
}

/// Condition-adjusted rate and single-batch ceiling shared by resolution and planning.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PoweredOreEquipmentLimits {
    processing_rate: MassFlow,
    maximum_batch_mass: Mass,
}

impl PoweredOreEquipmentLimits {
    #[must_use]
    pub(super) const fn processing_rate(self) -> MassFlow {
        self.processing_rate
    }

    #[must_use]
    pub(super) const fn maximum_batch_mass(self) -> Mass {
        self.maximum_batch_mass
    }
}

pub(super) fn classify_powered_ore_bottleneck(
    throughput_duration: TickSpan,
    energy_duration: TickSpan,
) -> PoweredOreBottleneck {
    match throughput_duration.cmp(&energy_duration) {
        std::cmp::Ordering::Greater => PoweredOreBottleneck::Throughput,
        std::cmp::Ordering::Less => PoweredOreBottleneck::EnergyDelivery,
        std::cmp::Ordering::Equal => PoweredOreBottleneck::Balanced,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PoweredOreJobReplay {
    processing_rate: MassFlow,
    traced_carrier: EnergyCarrier,
    traced_energy: Energy,
    required_carrier: EnergyCarrier,
    required_energy: Energy,
    available_power: Power,
    condition_before: Condition,
    condition_wear_ppm_per_active_tick: u32,
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
    let limits = resolve_powered_ore_equipment_limits(
        equipment,
        condition_before,
        mass_flow_capability,
        maximum_batch_mass_capability,
    )?;
    if selected_mass > limits.maximum_batch_mass() {
        return Err(PoweredOreEquipmentError::BatchMassExceeded {
            selected: selected_mass,
            maximum: limits.maximum_batch_mass(),
        });
    }
    Ok(PoweredOreEquipment {
        processing_rate: limits.processing_rate(),
    })
}

/// Resolves the condition-adjusted physical rate and batch ceiling without choosing a batch mass.
pub(super) fn resolve_powered_ore_equipment_limits(
    equipment: &EquipmentDefinition,
    condition_before: Condition,
    mass_flow_capability: CapabilityId,
    maximum_batch_mass_capability: CapabilityId,
) -> Result<PoweredOreEquipmentLimits, PoweredOreEquipmentError> {
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
    Ok(PoweredOreEquipmentLimits {
        processing_rate,
        maximum_batch_mass,
    })
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

/// Replays the common resource/equipment admission portion of a persisted powered ore job.
///
/// Callers intentionally validate their process-specific output snapshot after this phase and before
/// `validate_powered_ore_job_replay`, preserving canonical trusted-load error ordering.
pub(super) fn resolve_powered_ore_job_replay(
    registries: &Registries,
    job: &ProductionJobRecord,
    profile: PoweredOreProcessProfile,
) -> Result<PoweredOreJobReplay, PoweredOreJobValidationError> {
    let consumed_energy = job
        .consumed_energy()
        .ok_or(PoweredOreJobValidationError::MissingEnergy)?;
    if job.released_energy().is_some() {
        return Err(PoweredOreJobValidationError::UnexpectedReleasedEnergy);
    }
    let provider = job
        .equipment_provider()
        .ok_or(PoweredOreJobValidationError::MissingEquipmentProvider)?;
    let equipment_definition = registries
        .equipment()
        .get_equipment(provider.definition())
        .ok_or(PoweredOreJobValidationError::UnknownEquipmentDefinition)?;
    let energy_definition = registries
        .energy()
        .get_store(consumed_energy.definition())
        .ok_or(PoweredOreJobValidationError::UnknownEnergyDefinition)?;
    let powered_equipment = resolve_powered_ore_equipment(
        equipment_definition,
        provider.condition(),
        profile.mass_flow_capability(),
        profile.max_batch_mass_capability(),
        job.consumed_mass(),
    )
    .map_err(|error| match error {
        PoweredOreEquipmentError::MissingMassFlowCapability => {
            PoweredOreJobValidationError::MissingMassFlowCapability
        }
        PoweredOreEquipmentError::MissingMaximumBatchMassCapability => {
            PoweredOreJobValidationError::MissingMaximumBatchMassCapability
        }
        PoweredOreEquipmentError::BatchMassExceeded { selected, maximum } => {
            PoweredOreJobValidationError::BatchMassExceeded { selected, maximum }
        }
    })?;
    Ok(PoweredOreJobReplay {
        processing_rate: powered_equipment.processing_rate(),
        traced_carrier: consumed_energy.carrier(),
        traced_energy: consumed_energy.energy(),
        required_carrier: profile.energy_carrier(),
        required_energy: calculate_mass_specific_energy(
            job.consumed_mass(),
            profile.specific_energy(),
        ),
        available_power: energy_definition.max_output_power(),
        condition_before: provider.condition(),
        condition_wear_ppm_per_active_tick: profile.condition_wear_ppm_per_active_tick(),
    })
}

/// Validates common energy, duration, and wear replay after process-specific output validation.
pub(super) fn validate_powered_ore_job_replay(
    registries: &Registries,
    job: &ProductionJobRecord,
    replay: PoweredOreJobReplay,
) -> Result<(), PoweredOreJobValidationError> {
    if replay.traced_carrier != replay.required_carrier {
        return Err(PoweredOreJobValidationError::WrongEnergyCarrier {
            required: replay.required_carrier,
            provided: replay.traced_carrier,
        });
    }
    if replay.traced_energy != replay.required_energy {
        return Err(PoweredOreJobValidationError::EnergyMismatch {
            traced: replay.traced_energy,
            required: replay.required_energy,
        });
    }
    let timing = resolve_powered_ore_timing(
        registries,
        replay.processing_rate,
        job.consumed_mass(),
        replay.required_energy,
        replay.available_power,
        replay.condition_wear_ppm_per_active_tick,
        replay.condition_before,
    )
    .map_err(|error| match error {
        PoweredOreTimingError::Throughput(error) => {
            PoweredOreJobValidationError::ThroughputDuration(error)
        }
        PoweredOreTimingError::Energy(error) => PoweredOreJobValidationError::EnergyDuration(error),
        PoweredOreTimingError::Condition(error) => {
            PoweredOreJobValidationError::ConditionDuration(error)
        }
    })?;
    let required_duration = timing.duration();
    if job.active_duration() != required_duration {
        return Err(PoweredOreJobValidationError::DurationMismatch {
            stored_ticks: job.active_duration().value(),
            required_ticks: required_duration.value(),
        });
    }
    let stored_condition = job
        .equipment_condition_after()
        .ok_or(PoweredOreJobValidationError::MissingConditionOutcome)?;
    let required_condition = timing.condition_after();
    if stored_condition != required_condition {
        return Err(PoweredOreJobValidationError::ConditionOutcomeMismatch {
            stored: stored_condition,
            required: required_condition,
        });
    }
    Ok(())
}
