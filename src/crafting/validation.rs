//! Persistence replay validation for in-flight manual shaping jobs.

use std::num::NonZeroU64;

use crate::capability::CapabilityValue;
use crate::core::quantity::Temperature;
use crate::core::time::TickSpan;
use crate::equipment::resolve_equipment_capability;
use crate::maintenance::calculate_usable_condition_after_active_ticks;
use crate::material::MaterialLotSpec;
use crate::ore_processing::calculate_mass_flow_duration_ceiling;
use crate::production::{ProcessOutputStreamId, ProductionJobRecord};
use crate::registry::Registries;

use super::{
    ManualCraftDefinition, ManualCraftEquipmentProfile,
    batch::{ManualCraftBatchError, validate_manual_craft_batch},
};

mod error;

pub use error::ManualCraftJobValidationError;

fn validate_manual_craft_resources(
    registries: &Registries,
    definition: &ManualCraftDefinition,
    job: &ProductionJobRecord,
) -> Result<Option<TickSpan>, ManualCraftJobValidationError> {
    if job.consumed_energy().is_some() || job.released_energy().is_some() {
        return Err(ManualCraftJobValidationError::UnexpectedEnergy { job: job.id() });
    }
    let Some(provider) = job.equipment_provider() else {
        if job.equipment_condition_after().is_some() || job.has_required_active_support() {
            return Err(ManualCraftJobValidationError::UnexpectedEquipment { job: job.id() });
        }
        if definition
            .equipment_profile()
            .is_some_and(ManualCraftEquipmentProfile::requires_equipment)
        {
            return Err(ManualCraftJobValidationError::MissingRequiredEquipment { job: job.id() });
        }
        return Ok(None);
    };
    let profile = definition
        .equipment_profile()
        .ok_or(ManualCraftJobValidationError::UnexpectedEquipment { job: job.id() })?;
    let equipment_definition = registries
        .equipment()
        .get_equipment(provider.definition())
        .ok_or(ManualCraftJobValidationError::UnknownEquipmentDefinition {
            job: job.id(),
            definition: provider.definition(),
        })?;
    let capability = profile.mass_flow_capability();
    let rate = match resolve_equipment_capability(
        equipment_definition,
        provider.condition(),
        capability,
    ) {
        Some(CapabilityValue::MassFlow(rate)) => rate,
        Some(value) => {
            return Err(
                ManualCraftJobValidationError::EquipmentCapabilityKindMismatch {
                    job: job.id(),
                    capability,
                    found: value.kind(),
                },
            );
        }
        None => {
            return Err(ManualCraftJobValidationError::MissingEquipmentCapability {
                job: job.id(),
                capability,
            });
        }
    };
    let duration = calculate_mass_flow_duration_ceiling(
        rate,
        job.consumed_mass(),
        registries.core().physical_tick_duration(),
    )
    .map_err(|error| ManualCraftJobValidationError::EquipmentDuration {
        job: job.id(),
        error,
    })?;
    let required_condition = calculate_usable_condition_after_active_ticks(
        profile.condition_wear_ppm_per_active_tick(),
        provider.condition(),
        duration,
    )
    .map_err(|error| ManualCraftJobValidationError::EquipmentCondition {
        job: job.id(),
        error,
    })?;
    let stored_condition = job
        .equipment_condition_after()
        .ok_or(ManualCraftJobValidationError::UnexpectedEquipment { job: job.id() })?;
    if stored_condition != required_condition {
        return Err(ManualCraftJobValidationError::EquipmentConditionMismatch {
            job: job.id(),
            stored: stored_condition,
            required: required_condition,
        });
    }
    Ok(Some(duration))
}

fn reconstruct_manual_craft_outputs(
    definition: &ManualCraftDefinition,
    job: &ProductionJobRecord,
    batches: NonZeroU64,
    temperature: Temperature,
) -> Result<Vec<MaterialLotSpec>, ManualCraftJobValidationError> {
    let mut expected_outputs =
        super::batch::build_manual_craft_outputs(definition, batches, temperature).map_err(
            |error| match error {
                super::batch::ManualCraftOutputError::MassOverflow { batches, .. } => {
                    ManualCraftJobValidationError::OutputMassOverflow {
                        job: job.id(),
                        batches,
                    }
                }
                super::batch::ManualCraftOutputError::Construction { error, .. } => {
                    ManualCraftJobValidationError::OutputConstruction {
                        job: job.id(),
                        error,
                    }
                }
            },
        )?;
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
    let Some(definition) = definition else {
        return Ok(());
    };
    let batch = validate_manual_craft_batch(definition, job.consumed_mass(), job.consumed_inputs())
        .map_err(|error| match error {
            ManualCraftBatchError::EmptyInput => {
                ManualCraftJobValidationError::EmptyInput { job: job.id() }
            }
            ManualCraftBatchError::InputCommodityMismatch => {
                ManualCraftJobValidationError::InputCommodityMismatch { job: job.id() }
            }
            ManualCraftBatchError::InputCompositionMismatch => {
                ManualCraftJobValidationError::InputCompositionMismatch { job: job.id() }
            }
            ManualCraftBatchError::MixedInputTemperature => {
                ManualCraftJobValidationError::MixedInputTemperature { job: job.id() }
            }
            ManualCraftBatchError::InputMassNotWholeBatches {
                consumed,
                batch_mass,
            } => ManualCraftJobValidationError::InputMassNotWholeBatches {
                job: job.id(),
                consumed,
                batch_mass,
            },
        })?;
    let batches = batch.batches();
    let required_duration = match validate_manual_craft_resources(registries, definition, job)? {
        Some(duration) => duration,
        None => definition
            .duration()
            .value()
            .checked_mul(batches.get())
            .map(TickSpan::new)
            .ok_or(ManualCraftJobValidationError::DurationOverflow {
                job: job.id(),
                batches,
            })?,
    };
    if job.active_duration() != required_duration {
        return Err(ManualCraftJobValidationError::DurationMismatch {
            job: job.id(),
            stored: job.active_duration(),
            required: required_duration,
        });
    }
    let temperature = batch.temperature();
    let expected_outputs = reconstruct_manual_craft_outputs(definition, job, batches, temperature)?;
    validate_manual_craft_outputs(job, &expected_outputs)
}
