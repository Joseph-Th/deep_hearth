//! Trusted-load replay for direct-labor ore-processing resources and duration.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::capability::{CapabilityId, CapabilityValueKind};
use crate::core::throughput::MassFlowDurationError;
use crate::core::time::TickSpan;
use crate::equipment::{
    EquipmentMassFlowResolutionError, EquipmentMassFlowScheduleError,
    resolve_equipment_mass_flow_schedule,
};
use crate::maintenance::{ActiveConditionDurationError, Condition};
use crate::ore_processing::definitions::ManualOreProcessProfile;
use crate::production::ProductionJobRecord;
use crate::registry::Registries;

use super::{ManualOrePhysicsError, resolve_manual_ore_duration, validate_manual_ore_batch};

/// Persistent-state failure in the common direct-labor ore-processing envelope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManualOreJobValidationError {
    UnexpectedEnergy,
    UnexpectedEquipment,
    UnknownEquipmentDefinition,
    MissingEquipmentCapability {
        capability: CapabilityId,
    },
    EquipmentCapabilityKindMismatch {
        capability: CapabilityId,
        found: CapabilityValueKind,
    },
    EquipmentDuration(MassFlowDurationError),
    EquipmentCondition(ActiveConditionDurationError),
    MissingConditionOutcome,
    ConditionOutcomeMismatch {
        stored: Condition,
        required: Condition,
    },
    Physics(ManualOrePhysicsError),
    DurationMismatch {
        stored: TickSpan,
        required: TickSpan,
    },
}

impl Display for ManualOreJobValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnexpectedEnergy => {
                formatter.write_str("job carries unauthored stored-energy resources")
            }
            Self::UnexpectedEquipment => {
                formatter.write_str("job carries unauthored equipment resources")
            }
            Self::UnknownEquipmentDefinition => {
                formatter.write_str("job references an unknown equipment definition")
            }
            Self::MissingEquipmentCapability { capability } => write!(
                formatter,
                "job equipment lacks manual ore throughput capability {}",
                capability.value()
            ),
            Self::EquipmentCapabilityKindMismatch { capability, found } => write!(
                formatter,
                "job equipment capability {} has {found:?} rather than mass-flow semantics",
                capability.value()
            ),
            Self::EquipmentDuration(error) => {
                write!(
                    formatter,
                    "job equipment throughput duration failed: {error}"
                )
            }
            Self::EquipmentCondition(error) => {
                write!(
                    formatter,
                    "job equipment condition duration failed: {error}"
                )
            }
            Self::MissingConditionOutcome => {
                formatter.write_str("job has no persisted equipment-condition outcome")
            }
            Self::ConditionOutcomeMismatch { stored, required } => write!(
                formatter,
                "job stores equipment condition {} ppm but physics require {} ppm",
                stored.parts_per_million(),
                required.parts_per_million()
            ),
            Self::Physics(error) => write!(formatter, "{error}"),
            Self::DurationMismatch { stored, required } => write!(
                formatter,
                "job stores {} active ticks but requires {}",
                stored.value(),
                required.value()
            ),
        }
    }
}

impl Error for ManualOreJobValidationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Physics(error) => Some(error),
            Self::EquipmentDuration(error) => Some(error),
            Self::EquipmentCondition(error) => Some(error),
            Self::UnexpectedEnergy
            | Self::UnexpectedEquipment
            | Self::UnknownEquipmentDefinition
            | Self::MissingEquipmentCapability { .. }
            | Self::EquipmentCapabilityKindMismatch { .. }
            | Self::MissingConditionOutcome
            | Self::ConditionOutcomeMismatch { .. }
            | Self::DurationMismatch { .. } => None,
        }
    }
}

/// Validates common resources and batch size before process-specific output replay.
pub(in crate::ore_processing) fn validate_manual_ore_job_admission(
    registries: &Registries,
    job: &ProductionJobRecord,
    profile: ManualOreProcessProfile,
) -> Result<TickSpan, ManualOreJobValidationError> {
    if job.consumed_energy().is_some() || job.released_energy().is_some() {
        return Err(ManualOreJobValidationError::UnexpectedEnergy);
    }
    validate_manual_ore_batch(profile, job.consumed_mass())
        .map_err(ManualOreJobValidationError::Physics)?;
    let Some(provider) = job.equipment_provider() else {
        if job.equipment_condition_after().is_some() || job.has_required_active_support() {
            return Err(ManualOreJobValidationError::UnexpectedEquipment);
        }
        return resolve_manual_ore_duration(
            registries.core().physical_tick_duration(),
            profile,
            job.consumed_mass(),
        )
        .map_err(ManualOreJobValidationError::Physics);
    };
    let equipment_profile = profile
        .equipment_profile()
        .ok_or(ManualOreJobValidationError::UnexpectedEquipment)?;
    let definition = registries
        .equipment()
        .get_equipment(provider.definition())
        .ok_or(ManualOreJobValidationError::UnknownEquipmentDefinition)?;
    let schedule = resolve_equipment_mass_flow_schedule(
        definition,
        provider.condition(),
        equipment_profile.mass_flow_capability(),
        job.consumed_mass(),
        registries.core().physical_tick_duration(),
        equipment_profile.condition_wear_ppm_per_active_tick(),
    )
    .map_err(|error| match error {
        EquipmentMassFlowResolutionError::MissingCapability { capability } => {
            ManualOreJobValidationError::MissingEquipmentCapability { capability }
        }
        EquipmentMassFlowResolutionError::CapabilityKindMismatch { capability, found } => {
            ManualOreJobValidationError::EquipmentCapabilityKindMismatch { capability, found }
        }
        EquipmentMassFlowResolutionError::Schedule(error) => match error {
            EquipmentMassFlowScheduleError::Duration(error) => {
                ManualOreJobValidationError::EquipmentDuration(error)
            }
            EquipmentMassFlowScheduleError::Condition(error) => {
                ManualOreJobValidationError::EquipmentCondition(error)
            }
        },
    })?;
    let stored_condition = job
        .equipment_condition_after()
        .ok_or(ManualOreJobValidationError::MissingConditionOutcome)?;
    if stored_condition != schedule.condition_after() {
        return Err(ManualOreJobValidationError::ConditionOutcomeMismatch {
            stored: stored_condition,
            required: schedule.condition_after(),
        });
    }
    Ok(schedule.duration())
}

/// Replays common duration physics after process-specific outputs have been validated.
pub(in crate::ore_processing) fn validate_manual_ore_job_duration(
    job: &ProductionJobRecord,
    required: TickSpan,
) -> Result<(), ManualOreJobValidationError> {
    if job.active_duration() != required {
        return Err(ManualOreJobValidationError::DurationMismatch {
            stored: job.active_duration(),
            required,
        });
    }
    Ok(())
}
