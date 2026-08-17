//! Persistence replay validation for in-flight manual shaping jobs.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::time::TickSpan;
use crate::material::{MaterialComposition, MaterialLotSpec, MaterialLotSpecError};
use crate::production::{ProcessOutputStreamId, ProductionJobId, ProductionJobRecord};
use crate::registry::Registries;

/// Corruption or semantic drift in an in-flight manual shaping job.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManualCraftJobValidationError {
    UnexpectedEnergy {
        job: ProductionJobId,
    },
    UnexpectedEquipment {
        job: ProductionJobId,
    },
    InputCommodityMismatch {
        job: ProductionJobId,
    },
    InputCompositionMismatch {
        job: ProductionJobId,
    },
    MixedInputTemperature {
        job: ProductionJobId,
    },
    DurationMismatch {
        job: ProductionJobId,
        stored: TickSpan,
        authored: TickSpan,
    },
    OutputConstruction {
        job: ProductionJobId,
        error: MaterialLotSpecError,
    },
    OutputMismatch {
        job: ProductionJobId,
    },
}

impl Display for ManualCraftJobValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnexpectedEnergy { job } => write!(
                formatter,
                "manual craft job {} carries energy despite having no authored energy resource",
                job.value()
            ),
            Self::UnexpectedEquipment { job } => write!(
                formatter,
                "manual craft job {} carries equipment despite being authored as hand work",
                job.value()
            ),
            Self::InputCommodityMismatch { job } => write!(
                formatter,
                "manual craft job {} consumed a commodity outside its authored hand recipe",
                job.value()
            ),
            Self::InputCompositionMismatch { job } => write!(
                formatter,
                "manual craft job {} consumed non-pure material that its hand-shaping resolver cannot transform",
                job.value()
            ),
            Self::MixedInputTemperature { job } => write!(
                formatter,
                "manual craft job {} combines different input temperatures without thermal physics",
                job.value()
            ),
            Self::DurationMismatch {
                job,
                stored,
                authored,
            } => write!(
                formatter,
                "manual craft job {} stores {} active ticks but authored hand work requires {}",
                job.value(),
                stored.value(),
                authored.value()
            ),
            Self::OutputConstruction { job, error } => write!(
                formatter,
                "manual craft job {} authored output cannot be reconstructed: {error}",
                job.value()
            ),
            Self::OutputMismatch { job } => write!(
                formatter,
                "manual craft job {} output snapshot disagrees with authored shaping semantics",
                job.value()
            ),
        }
    }
}

impl Error for ManualCraftJobValidationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::OutputConstruction { error, .. } => Some(error),
            Self::UnexpectedEnergy { job: _ }
            | Self::UnexpectedEquipment { job: _ }
            | Self::InputCommodityMismatch { job: _ }
            | Self::InputCompositionMismatch { job: _ }
            | Self::MixedInputTemperature { job: _ }
            | Self::DurationMismatch { .. }
            | Self::OutputMismatch { job: _ } => None,
        }
    }
}

pub(crate) fn validate_loaded_manual_craft_job(
    registries: &Registries,
    job: &ProductionJobRecord,
) -> Result<(), ManualCraftJobValidationError> {
    let Some(definition) = registries.crafting().get_manual(job.process()) else {
        return Ok(());
    };
    if job.consumed_energy().is_some() || job.released_energy().is_some() {
        return Err(ManualCraftJobValidationError::UnexpectedEnergy { job: job.id() });
    }
    if job.equipment_provider().is_some()
        || job.equipment_condition_after().is_some()
        || job.has_required_active_support()
    {
        return Err(ManualCraftJobValidationError::UnexpectedEquipment { job: job.id() });
    }
    if job.active_duration() != definition.duration() {
        return Err(ManualCraftJobValidationError::DurationMismatch {
            job: job.id(),
            stored: job.active_duration(),
            authored: definition.duration(),
        });
    }

    let expected_composition = MaterialComposition::pure(definition.input().material());
    let mut temperature = None;
    for trace in job.consumed_inputs() {
        if trace.profile().commodity() != definition.input() {
            return Err(ManualCraftJobValidationError::InputCommodityMismatch { job: job.id() });
        }
        if trace.profile().composition() != &expected_composition {
            return Err(ManualCraftJobValidationError::InputCompositionMismatch { job: job.id() });
        }
        match temperature {
            Some(existing) if existing != trace.profile().temperature() => {
                return Err(ManualCraftJobValidationError::MixedInputTemperature { job: job.id() });
            }
            Some(_) => {}
            None => temperature = Some(trace.profile().temperature()),
        }
    }
    let Some(temperature) = temperature else {
        return Err(ManualCraftJobValidationError::InputCommodityMismatch { job: job.id() });
    };

    let mut expected_outputs = definition
        .outputs()
        .iter()
        .map(|output| {
            MaterialLotSpec::with_composition(
                output.commodity(),
                output.mass(),
                temperature,
                MaterialComposition::pure(output.commodity().material()),
            )
            .map_err(|error| ManualCraftJobValidationError::OutputConstruction {
                job: job.id(),
                error,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    expected_outputs.sort();

    let Some(stream) = job.single_output_stream() else {
        return Err(ManualCraftJobValidationError::OutputMismatch { job: job.id() });
    };
    if stream.id() != ProcessOutputStreamId::PRIMARY || stream.outputs() != expected_outputs {
        return Err(ManualCraftJobValidationError::OutputMismatch { job: job.id() });
    }
    Ok(())
}
