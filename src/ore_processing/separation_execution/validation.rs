//! Exhaustive replay validation for persisted constituent-separation production jobs.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::production::{ProductionJobId, ProductionJobRecord};
use crate::registry::Registries;

use super::{ConstituentSeparationBatchError, resolve_separation_outputs};
use crate::ore_processing::ManualOreJobValidationError;
use crate::ore_processing::manual_physics::{
    validate_manual_ore_job_admission, validate_manual_ore_job_duration,
};
use crate::ore_processing::powered_physics::{
    PoweredOreJobValidationError, resolve_powered_ore_job_replay, validate_powered_ore_job_replay,
};
use crate::ore_processing::{
    ConstituentSeparationProcessDefinition, ManualConstituentSeparationProcessDefinition,
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
    Manual {
        job: ProductionJobId,
        error: ManualOreJobValidationError,
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
            Self::Manual { job, error } => write!(
                formatter,
                "manual constituent-separation job {} replay failed: {error}",
                job.value(),
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
            Self::Manual { error, .. } => Some(error),
            Self::OutputMismatch { .. } => None,
        }
    }
}

fn validate_output_streams(
    job: &ProductionJobRecord,
    target: &[crate::material::MaterialLotSpec],
    residue: &[crate::material::MaterialLotSpec],
) -> Result<(), ConstituentSeparationJobValidationError> {
    if job.output_streams().len() != 2 {
        return Err(ConstituentSeparationJobValidationError::OutputMismatch { job: job.id() });
    }
    for (stream_id, outputs) in [
        (
            ConstituentSeparationProcessDefinition::TARGET_STREAM,
            target,
        ),
        (
            ConstituentSeparationProcessDefinition::RESIDUE_STREAM,
            residue,
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
    Ok(())
}

fn validate_loaded_manual_separation_job(
    registries: &Registries,
    job: &ProductionJobRecord,
    definition: ManualConstituentSeparationProcessDefinition,
) -> Result<(), ConstituentSeparationJobValidationError> {
    validate_manual_ore_job_admission(job, definition.operating_profile()).map_err(|error| {
        ConstituentSeparationJobValidationError::Manual {
            job: job.id(),
            error,
        }
    })?;
    let target_particle_size_policy = registries
        .materials()
        .get_form(definition.target_output_form())
        .unwrap_or_else(|| unreachable!("validated manual separation target form disappeared"))
        .particle_size_policy();
    let expected = resolve_separation_outputs(
        registries.materials(),
        definition.physics(),
        target_particle_size_policy,
        job.consumed_inputs(),
    )
    .map_err(|error| ConstituentSeparationJobValidationError::Batch {
        job: job.id(),
        error,
    })?;
    validate_output_streams(job, &expected.target, &expected.residue)?;
    validate_manual_ore_job_duration(
        registries.core().physical_tick_duration(),
        job,
        definition.operating_profile(),
    )
    .map_err(|error| ConstituentSeparationJobValidationError::Manual {
        job: job.id(),
        error,
    })
}

pub(crate) fn validate_loaded_constituent_separation_job(
    registries: &Registries,
    job: &ProductionJobRecord,
) -> Result<(), ConstituentSeparationJobValidationError> {
    let definition = registries
        .ore_processing()
        .get_constituent_separation(job.process());
    if let Some(manual) = registries
        .ore_processing()
        .get_manual_constituent_separation(job.process())
    {
        return validate_loaded_manual_separation_job(registries, job, manual);
    }
    let Some(definition) = definition else {
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
        definition.physics(),
        target_particle_size_policy,
        job.consumed_inputs(),
    )
    .map_err(|error| ConstituentSeparationJobValidationError::Batch {
        job: job.id(),
        error,
    })?;
    validate_output_streams(job, &expected.target, &expected.residue)?;
    validate_powered_ore_job_replay(registries, job, replay).map_err(|error| {
        ConstituentSeparationJobValidationError::Powered {
            job: job.id(),
            error,
        }
    })
}
