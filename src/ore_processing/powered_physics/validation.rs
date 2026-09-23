//! Trusted-load replay for shared powered ore-processing equipment, energy, timing, and wear.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::capability::CapabilityEvaluationError;
use crate::core::quantity::{Energy, Mass, MassFlow, Power};
use crate::core::throughput::MassFlowDurationError;
use crate::energy::{EnergyCarrier, PowerDurationError, calculate_mass_specific_energy};
use crate::maintenance::{ActiveConditionDurationError, Condition};
use crate::production::ProductionJobRecord;
use crate::registry::Registries;

use crate::ore_processing::PoweredOreProcessProfile;

use super::{
    PoweredOreEquipmentError, PoweredOreTimingError, resolve_powered_ore_equipment,
    resolve_powered_ore_timing, validate_powered_ore_process_capabilities,
};

/// Corruption or authored-physics drift shared by every persisted powered ore-processing job.
///
/// Process-specific output replay remains in the owning process module. This error owns only the
/// common finite-energy equipment, throughput, timing, and wear contract so those process families
/// cannot silently diverge during trusted-load validation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PoweredOreJobValidationError {
    MissingEnergy,
    UnexpectedReleasedEnergy,
    MissingEquipmentProvider,
    UnknownEquipmentDefinition,
    UnknownEnergyDefinition,
    Capability(CapabilityEvaluationError),
    MissingMassFlowCapability,
    MissingMaximumBatchMassCapability,
    BatchMassExceeded {
        selected: Mass,
        maximum: Mass,
    },
    WrongEnergyCarrier {
        required: EnergyCarrier,
        provided: EnergyCarrier,
    },
    EnergyMismatch {
        traced: Energy,
        required: Energy,
    },
    ThroughputDuration(MassFlowDurationError),
    EnergyDuration(PowerDurationError),
    ConditionDuration(ActiveConditionDurationError),
    DurationMismatch {
        stored_ticks: u64,
        required_ticks: u64,
    },
    MissingConditionOutcome,
    ConditionOutcomeMismatch {
        stored: Condition,
        required: Condition,
    },
}

impl Display for PoweredOreJobValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingEnergy => formatter.write_str("missing consumed work-energy trace"),
            Self::UnexpectedReleasedEnergy => formatter
                .write_str("contains released energy not authorized by powered ore processing"),
            Self::MissingEquipmentProvider => {
                formatter.write_str("missing occupied equipment provider")
            }
            Self::UnknownEquipmentDefinition => {
                formatter.write_str("references an unknown equipment definition")
            }
            Self::UnknownEnergyDefinition => {
                formatter.write_str("references an unknown energy-store definition")
            }
            Self::Capability(error) => write!(
                formatter,
                "equipment fails authored process capability requirements: {error}"
            ),
            Self::MissingMassFlowCapability => {
                formatter.write_str("equipment lacks the authored mass-flow capability")
            }
            Self::MissingMaximumBatchMassCapability => {
                formatter.write_str("equipment lacks the authored maximum-batch capability")
            }
            Self::BatchMassExceeded { selected, maximum } => write!(
                formatter,
                "selected {} mg above the traced equipment maximum {} mg",
                selected.milligrams(),
                maximum.milligrams()
            ),
            Self::WrongEnergyCarrier { required, provided } => write!(
                formatter,
                "requires {required:?} energy but traces {provided:?}"
            ),
            Self::EnergyMismatch { traced, required } => write!(
                formatter,
                "traces {} nJ but mass-specific work requires {} nJ",
                traced.nanojoules(),
                required.nanojoules()
            ),
            Self::ThroughputDuration(error) => {
                write!(formatter, "cannot recompute throughput duration: {error}")
            }
            Self::EnergyDuration(error) => write!(
                formatter,
                "cannot recompute energy-delivery duration: {error}"
            ),
            Self::ConditionDuration(error) => {
                write!(formatter, "exceeds equipment condition lifetime: {error}")
            }
            Self::DurationMismatch {
                stored_ticks,
                required_ticks,
            } => write!(
                formatter,
                "stores duration {stored_ticks} ticks but physics require {required_ticks}"
            ),
            Self::MissingConditionOutcome => {
                formatter.write_str("has no persisted equipment-condition outcome")
            }
            Self::ConditionOutcomeMismatch { stored, required } => write!(
                formatter,
                "stores equipment condition {} ppm but physics require {} ppm",
                stored.parts_per_million(),
                required.parts_per_million()
            ),
        }
    }
}

impl Error for PoweredOreJobValidationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Capability(error) => Some(error),
            Self::ThroughputDuration(error) => Some(error),
            Self::EnergyDuration(error) => Some(error),
            Self::ConditionDuration(error) => Some(error),
            Self::MissingEnergy
            | Self::UnexpectedReleasedEnergy
            | Self::MissingEquipmentProvider
            | Self::UnknownEquipmentDefinition
            | Self::UnknownEnergyDefinition
            | Self::MissingMassFlowCapability
            | Self::MissingMaximumBatchMassCapability
            | Self::BatchMassExceeded { .. }
            | Self::WrongEnergyCarrier { .. }
            | Self::EnergyMismatch { .. }
            | Self::DurationMismatch { .. }
            | Self::MissingConditionOutcome
            | Self::ConditionOutcomeMismatch { .. } => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ore_processing) struct PoweredOreJobReplay {
    processing_rate: MassFlow,
    traced_carrier: EnergyCarrier,
    traced_energy: Energy,
    required_carrier: EnergyCarrier,
    required_energy: Energy,
    available_power: Power,
    condition_before: Condition,
    condition_wear_ppm_per_active_tick: u32,
}

/// Replays the common resource/equipment admission portion of a persisted powered ore job.
///
/// Callers intentionally validate their process-specific output snapshot after this phase and before
/// `validate_powered_ore_job_replay`, preserving canonical trusted-load error ordering.
pub(in crate::ore_processing) fn resolve_powered_ore_job_replay(
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
    validate_powered_ore_process_capabilities(
        registries,
        job.process(),
        equipment_definition,
        provider.condition(),
    )
    .map_err(PoweredOreJobValidationError::Capability)?;
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
pub(in crate::ore_processing) fn validate_powered_ore_job_replay(
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
