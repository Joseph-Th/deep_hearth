//! Persistence replay validation for in-flight manual shaping jobs.

use std::error::Error;
use std::fmt::{Display, Formatter};
use std::num::NonZeroU64;

use crate::core::quantity::{Mass, Temperature};
use crate::core::time::TickSpan;
use crate::material::{MaterialComposition, MaterialLotSpec, MaterialLotSpecError};
use crate::production::{
    ProcessOutputStreamId, ProductionJobId, ProductionJobRecord, ProductionSuspensionReason,
};
use crate::registry::Registries;

use super::ManualCraftDefinition;

/// Corruption or semantic drift in an in-flight manual shaping job.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManualCraftJobValidationError {
    UnexpectedEnergy {
        job: ProductionJobId,
    },
    UnexpectedEquipment {
        job: ProductionJobId,
    },
    PlayerLaborSuspensionWithoutManualCraft {
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
    InputMassNotWholeBatches {
        job: ProductionJobId,
        consumed: Mass,
        batch_mass: Mass,
    },
    DurationOverflow {
        job: ProductionJobId,
        batches: NonZeroU64,
    },
    DurationMismatch {
        job: ProductionJobId,
        stored: TickSpan,
        required: TickSpan,
    },
    OutputMassOverflow {
        job: ProductionJobId,
        batches: NonZeroU64,
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
            Self::PlayerLaborSuspensionWithoutManualCraft { job } => write!(
                formatter,
                "production job {} claims unavailable player labor despite not being authored as manual crafting",
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
            Self::InputMassNotWholeBatches {
                job,
                consumed,
                batch_mass,
            } => write!(
                formatter,
                "manual craft job {} consumed {} mg, which is not a whole number of {} mg authored batches",
                job.value(),
                consumed.milligrams(),
                batch_mass.milligrams()
            ),
            Self::DurationOverflow { job, batches } => write!(
                formatter,
                "manual craft job {} repeats {} batches beyond the authoritative duration range",
                job.value(),
                batches.get()
            ),
            Self::DurationMismatch {
                job,
                stored,
                required,
            } => write!(
                formatter,
                "manual craft job {} stores {} active ticks but its repeated hand work requires {}",
                job.value(),
                stored.value(),
                required.value()
            ),
            Self::OutputMassOverflow { job, batches } => write!(
                formatter,
                "manual craft job {} output mass overflows when replaying {} batches",
                job.value(),
                batches.get()
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
            | Self::PlayerLaborSuspensionWithoutManualCraft { job: _ }
            | Self::InputCommodityMismatch { job: _ }
            | Self::InputCompositionMismatch { job: _ }
            | Self::MixedInputTemperature { job: _ }
            | Self::InputMassNotWholeBatches { .. }
            | Self::DurationOverflow { .. }
            | Self::DurationMismatch { .. }
            | Self::OutputMassOverflow { .. }
            | Self::OutputMismatch { job: _ } => None,
        }
    }
}

fn validate_manual_craft_resources(
    job: &ProductionJobRecord,
) -> Result<(), ManualCraftJobValidationError> {
    if job.consumed_energy().is_some() || job.released_energy().is_some() {
        return Err(ManualCraftJobValidationError::UnexpectedEnergy { job: job.id() });
    }
    if job.equipment_provider().is_some()
        || job.equipment_condition_after().is_some()
        || job.has_required_active_support()
    {
        return Err(ManualCraftJobValidationError::UnexpectedEquipment { job: job.id() });
    }
    Ok(())
}

fn validate_manual_craft_repetition(
    definition: &ManualCraftDefinition,
    job: &ProductionJobRecord,
) -> Result<NonZeroU64, ManualCraftJobValidationError> {
    let batch_mass = definition.input_mass();
    let consumed_mass = job.consumed_mass();
    let quotient = consumed_mass.milligrams() / batch_mass.milligrams();
    let remainder = consumed_mass.milligrams() % batch_mass.milligrams();
    let Some(batches) = NonZeroU64::new(quotient) else {
        return Err(ManualCraftJobValidationError::InputMassNotWholeBatches {
            job: job.id(),
            consumed: consumed_mass,
            batch_mass,
        });
    };
    if remainder != 0 {
        return Err(ManualCraftJobValidationError::InputMassNotWholeBatches {
            job: job.id(),
            consumed: consumed_mass,
            batch_mass,
        });
    }
    let required_duration = definition
        .duration()
        .value()
        .checked_mul(batches.get())
        .map(TickSpan::new)
        .ok_or(ManualCraftJobValidationError::DurationOverflow {
            job: job.id(),
            batches,
        })?;
    if job.active_duration() != required_duration {
        return Err(ManualCraftJobValidationError::DurationMismatch {
            job: job.id(),
            stored: job.active_duration(),
            required: required_duration,
        });
    }
    Ok(batches)
}

fn validate_manual_craft_inputs(
    definition: &ManualCraftDefinition,
    job: &ProductionJobRecord,
) -> Result<Temperature, ManualCraftJobValidationError> {
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
    temperature.ok_or(ManualCraftJobValidationError::InputCommodityMismatch { job: job.id() })
}

fn reconstruct_manual_craft_outputs(
    definition: &ManualCraftDefinition,
    job: &ProductionJobRecord,
    batches: NonZeroU64,
    temperature: Temperature,
) -> Result<Vec<MaterialLotSpec>, ManualCraftJobValidationError> {
    let mut expected_outputs = definition
        .outputs()
        .iter()
        .map(|output| {
            let mass = output
                .mass()
                .milligrams()
                .checked_mul(batches.get())
                .map(Mass::from_milligrams)
                .ok_or(ManualCraftJobValidationError::OutputMassOverflow {
                    job: job.id(),
                    batches,
                })?;
            MaterialLotSpec::with_composition(
                output.commodity(),
                mass,
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
    Ok(expected_outputs)
}

fn validate_manual_craft_outputs(
    job: &ProductionJobRecord,
    expected_outputs: &[MaterialLotSpec],
) -> Result<(), ManualCraftJobValidationError> {
    let Some(stream) = job.single_output_stream() else {
        return Err(ManualCraftJobValidationError::OutputMismatch { job: job.id() });
    };
    if stream.id() != ProcessOutputStreamId::PRIMARY || stream.outputs() != expected_outputs {
        return Err(ManualCraftJobValidationError::OutputMismatch { job: job.id() });
    }
    Ok(())
}

pub(crate) fn validate_loaded_manual_craft_job(
    registries: &Registries,
    job: &ProductionJobRecord,
) -> Result<(), ManualCraftJobValidationError> {
    let definition = registries.crafting().get_manual(job.process());
    if job.suspension().is_some_and(|suspension| {
        suspension.reason() == ProductionSuspensionReason::PlayerLaborUnavailable
    }) && definition.is_none()
    {
        return Err(
            ManualCraftJobValidationError::PlayerLaborSuspensionWithoutManualCraft {
                job: job.id(),
            },
        );
    }
    let Some(definition) = definition else {
        return Ok(());
    };
    validate_manual_craft_resources(job)?;
    let batches = validate_manual_craft_repetition(definition, job)?;
    let temperature = validate_manual_craft_inputs(definition, job)?;
    let expected_outputs = reconstruct_manual_craft_outputs(definition, job, batches, temperature)?;
    validate_manual_craft_outputs(job, &expected_outputs)
}
