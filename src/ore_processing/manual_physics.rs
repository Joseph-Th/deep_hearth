//! Shared direct-labor ore-processing throughput and persisted-job physics.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::throughput::{MassFlowDurationError, calculate_mass_flow_duration_ceiling};
use crate::core::time::{PhysicalTickDuration, TickSpan};
use crate::production::ProductionJobRecord;

use super::ManualOreProcessProfile;

/// Shared physical failure for one direct-labor ore-processing batch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManualOrePhysicsError {
    BatchMassExceeded { selected: Mass, maximum: Mass },
    ThroughputDuration(MassFlowDurationError),
}

impl Display for ManualOrePhysicsError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BatchMassExceeded { selected, maximum } => write!(
                formatter,
                "selected manual ore-processing batch {} mg exceeds maximum {} mg",
                selected.milligrams(),
                maximum.milligrams()
            ),
            Self::ThroughputDuration(error) => {
                write!(formatter, "manual ore-processing duration failed: {error}")
            }
        }
    }
}

impl Error for ManualOrePhysicsError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::ThroughputDuration(error) => Some(error),
            Self::BatchMassExceeded { .. } => None,
        }
    }
}

/// Persistent-state failure in the common direct-labor ore-processing envelope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManualOreJobValidationError {
    UnexpectedEnergy,
    UnexpectedEquipment,
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
            Self::UnexpectedEnergy | Self::UnexpectedEquipment | Self::DurationMismatch { .. } => {
                None
            }
        }
    }
}

/// Rejects a selected mass outside the authored manual processing envelope.
pub(super) fn validate_manual_ore_batch(
    profile: ManualOreProcessProfile,
    selected: Mass,
) -> Result<(), ManualOrePhysicsError> {
    if selected > profile.max_batch_mass() {
        return Err(ManualOrePhysicsError::BatchMassExceeded {
            selected,
            maximum: profile.max_batch_mass(),
        });
    }
    Ok(())
}

/// Resolves exact whole-tick attention time from the shared manual throughput profile.
pub(crate) fn resolve_manual_ore_duration(
    physical_tick_duration: PhysicalTickDuration,
    profile: ManualOreProcessProfile,
    selected: Mass,
) -> Result<TickSpan, ManualOrePhysicsError> {
    calculate_mass_flow_duration_ceiling(
        profile.processing_rate(),
        selected,
        physical_tick_duration,
    )
    .map_err(ManualOrePhysicsError::ThroughputDuration)
}

/// Validates common resource absence and batch size before process-specific output replay.
pub(super) fn validate_manual_ore_job_admission(
    job: &ProductionJobRecord,
    profile: ManualOreProcessProfile,
) -> Result<(), ManualOreJobValidationError> {
    if job.consumed_energy().is_some() || job.released_energy().is_some() {
        return Err(ManualOreJobValidationError::UnexpectedEnergy);
    }
    if job.equipment_provider().is_some()
        || job.equipment_condition_after().is_some()
        || job.has_required_active_support()
    {
        return Err(ManualOreJobValidationError::UnexpectedEquipment);
    }
    validate_manual_ore_batch(profile, job.consumed_mass())
        .map_err(ManualOreJobValidationError::Physics)
}

/// Replays common duration physics after process-specific outputs have been validated.
pub(super) fn validate_manual_ore_job_duration(
    physical_tick_duration: PhysicalTickDuration,
    job: &ProductionJobRecord,
    profile: ManualOreProcessProfile,
) -> Result<(), ManualOreJobValidationError> {
    let required =
        resolve_manual_ore_duration(physical_tick_duration, profile, job.consumed_mass())
            .map_err(ManualOreJobValidationError::Physics)?;
    if job.active_duration() != required {
        return Err(ManualOreJobValidationError::DurationMismatch {
            stored: job.active_duration(),
            required,
        });
    }
    Ok(())
}
