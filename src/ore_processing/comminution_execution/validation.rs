//! Exhaustive replay validation for persisted manual and powered comminution jobs.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::production::{ProductionJobId, ProductionJobRecord};
use crate::registry::Registries;

use crate::ore_processing::ManualOreJobValidationError;
use crate::ore_processing::manual_physics::{
    validate_manual_ore_job_admission, validate_manual_ore_job_duration,
};
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
    Manual {
        job: ProductionJobId,
        error: ManualOreJobValidationError,
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
            Self::Manual { job, error } => write!(
                formatter,
                "manual comminution job {} replay failed: {error}",
                job.value(),
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
            Self::Manual { error, .. } => Some(error),
            Self::OutputMismatch { .. } => None,
        }
    }
}

fn validate_loaded_manual_comminution_job(
    registries: &Registries,
    job: &ProductionJobRecord,
    definition: &crate::ore_processing::ManualComminutionProcessDefinition,
) -> Result<(), ComminutionJobValidationError> {
    let required_duration =
        validate_manual_ore_job_admission(registries, job, definition.operating_profile())
            .map_err(|error| ComminutionJobValidationError::Manual {
                job: job.id(),
                error,
            })?;
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
    validate_manual_ore_job_duration(job, required_duration).map_err(|error| {
        ComminutionJobValidationError::Manual {
            job: job.id(),
            error,
        }
    })
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
