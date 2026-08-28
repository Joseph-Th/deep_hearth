//! Exhaustive replay validation for persisted constituent-separation production jobs.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::production::{ProductionJobId, ProductionJobRecord};
use crate::registry::Registries;

use super::{ConstituentSeparationBatchError, resolve_separation_outputs};
use crate::ore_processing::ConstituentSeparationProcessDefinition;
use crate::ore_processing::powered_physics::{
    PoweredOreJobValidationError, resolve_powered_ore_job_replay, validate_powered_ore_job_replay,
};

/// Persistent-state failure found while replaying an in-flight constituent-separation job.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConstituentSeparationJobValidationError {
    Powered {
        job: ProductionJobId,
        error: PoweredOreJobValidationError,
    },
    Batch {
        job: ProductionJobId,
        error: ConstituentSeparationBatchError,
    },
    OutputMismatch {
        job: ProductionJobId,
    },
}

impl Display for ConstituentSeparationJobValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Powered { job, error } => write!(
                formatter,
                "constituent-separation job {} powered-physics replay failed: {error}",
                job.value()
            ),
            Self::Batch { job, error } => write!(
                formatter,
                "constituent-separation job {} input replay failed: {error}",
                job.value()
            ),
            Self::OutputMismatch { job } => write!(
                formatter,
                "constituent-separation job {} output streams do not match composition replay",
                job.value()
            ),
        }
    }
}

impl Error for ConstituentSeparationJobValidationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Powered { error, .. } => Some(error),
            Self::Batch { error, .. } => Some(error),
            Self::OutputMismatch { .. } => None,
        }
    }
}

pub(crate) fn validate_loaded_constituent_separation_job(
    registries: &Registries,
    job: &ProductionJobRecord,
) -> Result<(), ConstituentSeparationJobValidationError> {
    let Some(definition) = registries
        .ore_processing()
        .get_constituent_separation(job.process())
    else {
        return Ok(());
    };
    let replay = resolve_powered_ore_job_replay(registries, job, definition.operating_profile())
        .map_err(|error| ConstituentSeparationJobValidationError::Powered {
            job: job.id(),
            error,
        })?;
    let target_particle_size_policy = registries
        .materials()
        .get_form(definition.target_output_form())
        .unwrap_or_else(|| {
            unreachable!("validated separation target output form must remain available")
        })
        .particle_size_policy();
    let expected = resolve_separation_outputs(
        registries.materials(),
        definition,
        target_particle_size_policy,
        job.consumed_inputs(),
    )
    .map_err(|error| ConstituentSeparationJobValidationError::Batch {
        job: job.id(),
        error,
    })?;
    if job.output_streams().len() != 2 {
        return Err(ConstituentSeparationJobValidationError::OutputMismatch { job: job.id() });
    }
    for (stream_id, outputs) in [
        (
            ConstituentSeparationProcessDefinition::TARGET_STREAM,
            expected.target.as_slice(),
        ),
        (
            ConstituentSeparationProcessDefinition::RESIDUE_STREAM,
            expected.residue.as_slice(),
        ),
    ] {
        let Some(stored) = job
            .output_streams()
            .iter()
            .find(|stream| stream.id() == stream_id)
        else {
            return Err(ConstituentSeparationJobValidationError::OutputMismatch { job: job.id() });
        };
        if stored.outputs() != outputs {
            return Err(ConstituentSeparationJobValidationError::OutputMismatch { job: job.id() });
        }
    }
    validate_powered_ore_job_replay(registries, job, replay).map_err(|error| {
        ConstituentSeparationJobValidationError::Powered {
            job: job.id(),
            error,
        }
    })
}
