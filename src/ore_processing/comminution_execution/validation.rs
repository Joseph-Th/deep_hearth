//! Exhaustive replay validation for persisted manual and powered comminution jobs.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::throughput::MassFlowDurationError;
use crate::core::throughput::calculate_mass_flow_duration_ceiling;
use crate::core::time::TickSpan;
use crate::production::{ProductionJobId, ProductionJobRecord};
use crate::registry::Registries;

use crate::ore_processing::powered_physics::{
    PoweredOreJobValidationError, resolve_powered_ore_job_replay, validate_powered_ore_job_replay,
};

use super::ComminutionBatchError;
use super::outputs::{resolve_comminution_outputs, resolve_manual_comminution_outputs};

/// Persistent-state failure found while recomputing an in-flight comminution job from its traces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComminutionJobValidationError {
    Powered {
        job: ProductionJobId,
        error: PoweredOreJobValidationError,
    },
    Batch {
        job: ProductionJobId,
        error: ComminutionBatchError,
    },
    ManualUnexpectedEnergy {
        job: ProductionJobId,
    },
    ManualUnexpectedEquipment {
        job: ProductionJobId,
    },
    ManualBatchMassExceeded {
        job: ProductionJobId,
        selected: Mass,
        maximum: Mass,
    },
    ManualDuration {
        job: ProductionJobId,
        error: MassFlowDurationError,
    },
    ManualDurationMismatch {
        job: ProductionJobId,
        stored: TickSpan,
        required: TickSpan,
    },
    OutputMismatch {
        job: ProductionJobId,
    },
}

impl Display for ComminutionJobValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Powered { job, error } => write!(
                formatter,
                "comminution job {} powered-physics replay failed: {error}",
                job.value()
            ),
            Self::Batch { job, error } => write!(
                formatter,
                "comminution job {} has invalid batch physics: {error}",
                job.value()
            ),
            Self::ManualUnexpectedEnergy { job } => write!(
                formatter,
                "manual comminution job {} carries unauthored energy",
                job.value()
            ),
            Self::ManualUnexpectedEquipment { job } => write!(
                formatter,
                "manual comminution job {} carries unauthored equipment",
                job.value()
            ),
            Self::ManualBatchMassExceeded {
                job,
                selected,
                maximum,
            } => write!(
                formatter,
                "manual comminution job {} contains {} mg beyond its {} mg hand-breaking limit",
                job.value(),
                selected.milligrams(),
                maximum.milligrams()
            ),
            Self::ManualDuration { job, error } => write!(
                formatter,
                "manual comminution job {} duration replay failed: {error}",
                job.value()
            ),
            Self::ManualDurationMismatch {
                job,
                stored,
                required,
            } => write!(
                formatter,
                "manual comminution job {} stores {} active ticks but requires {}",
                job.value(),
                stored.value(),
                required.value()
            ),
            Self::OutputMismatch { job } => write!(
                formatter,
                "comminution job {} output snapshot no longer matches its consumed material traces",
                job.value()
            ),
        }
    }
}

impl Error for ComminutionJobValidationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Powered { error, .. } => Some(error),
            Self::Batch { error, .. } => Some(error),
            Self::ManualDuration { error, .. } => Some(error),
            Self::ManualUnexpectedEnergy { .. }
            | Self::ManualUnexpectedEquipment { .. }
            | Self::ManualBatchMassExceeded { .. }
            | Self::ManualDurationMismatch { .. }
            | Self::OutputMismatch { .. } => None,
        }
    }
}

fn validate_loaded_manual_comminution_job(
    registries: &Registries,
    job: &ProductionJobRecord,
    definition: &crate::ore_processing::ManualComminutionProcessDefinition,
) -> Result<(), ComminutionJobValidationError> {
    if job.consumed_energy().is_some() || job.released_energy().is_some() {
        return Err(ComminutionJobValidationError::ManualUnexpectedEnergy { job: job.id() });
    }
    if job.equipment_provider().is_some()
        || job.equipment_condition_after().is_some()
        || job.has_required_active_support()
    {
        return Err(ComminutionJobValidationError::ManualUnexpectedEquipment { job: job.id() });
    }
    if job.consumed_mass() > definition.max_batch_mass() {
        return Err(ComminutionJobValidationError::ManualBatchMassExceeded {
            job: job.id(),
            selected: job.consumed_mass(),
            maximum: definition.max_batch_mass(),
        });
    }
    let required_outputs = resolve_manual_comminution_outputs(definition, job.consumed_inputs())
        .map_err(|error| ComminutionJobValidationError::Batch {
            job: job.id(),
            error,
        })?;
    let Some(output_stream) = job.single_output_stream() else {
        return Err(ComminutionJobValidationError::OutputMismatch { job: job.id() });
    };
    if required_outputs.as_slice() != output_stream.outputs() {
        return Err(ComminutionJobValidationError::OutputMismatch { job: job.id() });
    }
    let required = calculate_mass_flow_duration_ceiling(
        definition.processing_rate(),
        job.consumed_mass(),
        registries.core().physical_tick_duration(),
    )
    .map_err(|error| ComminutionJobValidationError::ManualDuration {
        job: job.id(),
        error,
    })?;
    if job.active_duration() != required {
        return Err(ComminutionJobValidationError::ManualDurationMismatch {
            job: job.id(),
            stored: job.active_duration(),
            required,
        });
    }
    Ok(())
}

pub(crate) fn validate_loaded_comminution_job(
    registries: &Registries,
    job: &ProductionJobRecord,
) -> Result<(), ComminutionJobValidationError> {
    if let Some(definition) = registries
        .ore_processing()
        .get_manual_comminution(job.process())
    {
        return validate_loaded_manual_comminution_job(registries, job, definition);
    }
    let Some(definition) = registries.ore_processing().get_comminution(job.process()) else {
        return Ok(());
    };
    let replay = resolve_powered_ore_job_replay(registries, job, definition.operating_profile())
        .map_err(|error| ComminutionJobValidationError::Powered {
            job: job.id(),
            error,
        })?;
    let required_outputs =
        resolve_comminution_outputs(definition, job.consumed_inputs()).map_err(|error| {
            ComminutionJobValidationError::Batch {
                job: job.id(),
                error,
            }
        })?;
    let Some(output_stream) = job.single_output_stream() else {
        return Err(ComminutionJobValidationError::OutputMismatch { job: job.id() });
    };
    if required_outputs.as_slice() != output_stream.outputs() {
        return Err(ComminutionJobValidationError::OutputMismatch { job: job.id() });
    }
    validate_powered_ore_job_replay(registries, job, replay).map_err(|error| {
        ComminutionJobValidationError::Powered {
            job: job.id(),
            error,
        }
    })
}
