//! Persistence replay validation for in-flight manual shaping jobs.

use std::num::NonZeroU64;

use crate::core::quantity::Temperature;
use crate::core::time::TickSpan;
use crate::energy::calculate_mass_specific_energy;
use crate::material::MaterialLotSpec;
use crate::production::{ProcessOutputStreamId, ProductionJobRecord};
use crate::registry::Registries;

use super::powered::{PoweredCraftTimingError, resolve_powered_craft_rate};
use super::{
    ManualCraftDefinition, ManualCraftEquipmentProfile, PoweredCraftDefinition,
    batch::{ManualCraftBatchError, validate_manual_craft_batch},
    physics::{
        ManualCraftEquipmentResolutionError, ManualCraftEquipmentScheduleError,
        resolve_manual_craft_equipment_physics, resolve_manual_craft_hand_duration,
    },
};

mod error;

pub use error::CraftingJobValidationError;

fn validate_manual_craft_resources(
    registries: &Registries,
    definition: &ManualCraftDefinition,
    job: &ProductionJobRecord,
) -> Result<Option<TickSpan>, CraftingJobValidationError> {
    if job.consumed_energy().is_some() || job.released_energy().is_some() {
        return Err(CraftingJobValidationError::UnexpectedEnergy { job: job.id() });
    }
    let Some(provider) = job.equipment_provider() else {
        if job.equipment_condition_after().is_some() || job.has_required_active_support() {
            return Err(CraftingJobValidationError::UnexpectedEquipment { job: job.id() });
        }
        if definition
            .equipment_profile()
            .is_some_and(ManualCraftEquipmentProfile::requires_equipment)
        {
            return Err(CraftingJobValidationError::MissingRequiredEquipment { job: job.id() });
        }
        return Ok(None);
    };
    let profile = definition
        .equipment_profile()
        .ok_or(CraftingJobValidationError::UnexpectedEquipment { job: job.id() })?;
    let equipment_definition = registries
        .equipment()
        .get_equipment(provider.definition())
        .ok_or(CraftingJobValidationError::UnknownEquipmentDefinition {
            job: job.id(),
            definition: provider.definition(),
        })?;
    let schedule = resolve_manual_craft_equipment_physics(
        equipment_definition,
        provider.condition(),
        profile.mass_flow_capability(),
        job.consumed_mass(),
        registries.core().physical_tick_duration(),
        profile.condition_wear_ppm_per_active_tick(),
    )
    .map_err(|error| match error {
        ManualCraftEquipmentResolutionError::MissingCapability { capability } => {
            CraftingJobValidationError::MissingEquipmentCapability {
                job: job.id(),
                capability,
            }
        }
        ManualCraftEquipmentResolutionError::CapabilityKindMismatch { capability, found } => {
            CraftingJobValidationError::EquipmentCapabilityKindMismatch {
                job: job.id(),
                capability,
                found,
            }
        }
        ManualCraftEquipmentResolutionError::Schedule(error) => match error {
            ManualCraftEquipmentScheduleError::Duration(error) => {
                CraftingJobValidationError::EquipmentDuration {
                    job: job.id(),
                    error,
                }
            }
            ManualCraftEquipmentScheduleError::Condition(error) => {
                CraftingJobValidationError::EquipmentCondition {
                    job: job.id(),
                    error,
                }
            }
        },
    })?;
    let stored_condition = job
        .equipment_condition_after()
        .ok_or(CraftingJobValidationError::UnexpectedEquipment { job: job.id() })?;
    if stored_condition != schedule.condition_after() {
        return Err(CraftingJobValidationError::EquipmentConditionMismatch {
            job: job.id(),
            stored: stored_condition,
            required: schedule.condition_after(),
        });
    }
    Ok(Some(schedule.duration()))
}

fn validate_powered_craft_resources(
    registries: &Registries,
    definition: PoweredCraftDefinition,
    job: &ProductionJobRecord,
) -> Result<TickSpan, CraftingJobValidationError> {
    if job.released_energy().is_some() {
        return Err(CraftingJobValidationError::UnexpectedReleasedEnergy { job: job.id() });
    }
    let energy = job
        .consumed_energy()
        .ok_or(CraftingJobValidationError::MissingEnergy { job: job.id() })?;
    if energy.carrier() != definition.energy_carrier() {
        return Err(CraftingJobValidationError::EnergyCarrierMismatch {
            job: job.id(),
            stored: energy.carrier(),
            required: definition.energy_carrier(),
        });
    }
    let required_energy =
        calculate_mass_specific_energy(job.consumed_mass(), definition.specific_energy());
    if energy.energy() != required_energy {
        return Err(CraftingJobValidationError::EnergyAmountMismatch {
            job: job.id(),
            stored: energy.energy(),
            required: required_energy,
        });
    }
    let energy_definition = registries.energy().get_store(energy.definition()).ok_or(
        CraftingJobValidationError::UnknownEnergyDefinition {
            job: job.id(),
            definition: energy.definition(),
        },
    )?;
    if energy_definition.carrier() != definition.energy_carrier() {
        return Err(CraftingJobValidationError::EnergyCarrierMismatch {
            job: job.id(),
            stored: energy_definition.carrier(),
            required: definition.energy_carrier(),
        });
    }

    let provider = job
        .equipment_provider()
        .ok_or(CraftingJobValidationError::MissingRequiredEquipment { job: job.id() })?;
    let equipment_definition = registries
        .equipment()
        .get_equipment(provider.definition())
        .ok_or(CraftingJobValidationError::UnknownEquipmentDefinition {
            job: job.id(),
            definition: provider.definition(),
        })?;
    let rate = resolve_powered_craft_rate(
        registries,
        definition,
        equipment_definition,
        provider.condition(),
    )
    .map_err(
        |error| CraftingJobValidationError::PoweredEquipmentCapability {
            job: job.id(),
            error,
        },
    )?;
    let (duration, condition_after) = super::powered::resolve_powered_craft_timing(
        registries,
        rate,
        job.consumed_mass(),
        required_energy,
        energy_definition.max_output_power(),
        definition.condition_wear_ppm_per_active_tick(),
        provider.condition(),
    )
    .map_err(|error| match error {
        PoweredCraftTimingError::ThroughputDuration(error) => {
            CraftingJobValidationError::EquipmentDuration {
                job: job.id(),
                error,
            }
        }
        PoweredCraftTimingError::EnergyDuration(error) => {
            CraftingJobValidationError::EnergyDuration {
                job: job.id(),
                error,
            }
        }
        PoweredCraftTimingError::EquipmentCondition(error) => {
            CraftingJobValidationError::EquipmentCondition {
                job: job.id(),
                error,
            }
        }
    })?;
    let stored_condition = job
        .equipment_condition_after()
        .ok_or(CraftingJobValidationError::UnexpectedEquipment { job: job.id() })?;
    if stored_condition != condition_after {
        return Err(CraftingJobValidationError::EquipmentConditionMismatch {
            job: job.id(),
            stored: stored_condition,
            required: condition_after,
        });
    }
    Ok(duration)
}

fn reconstruct_manual_craft_outputs(
    definition: &ManualCraftDefinition,
    job: &ProductionJobRecord,
    batches: NonZeroU64,
    temperature: Temperature,
) -> Result<Vec<MaterialLotSpec>, CraftingJobValidationError> {
    let mut expected_outputs =
        super::batch::build_manual_craft_outputs(definition, batches, temperature).map_err(
            |error| match error {
                super::batch::ManualCraftOutputError::MassOverflow { batches, .. } => {
                    CraftingJobValidationError::OutputMassOverflow {
                        job: job.id(),
                        batches,
                    }
                }
                super::batch::ManualCraftOutputError::Construction { error, .. } => {
                    CraftingJobValidationError::OutputConstruction {
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
) -> Result<(), CraftingJobValidationError> {
    let Some(stream) = job.single_output_stream() else {
        return Err(CraftingJobValidationError::OutputMismatch { job: job.id() });
    };
    if stream.id() != ProcessOutputStreamId::PRIMARY || stream.outputs() != expected_outputs {
        return Err(CraftingJobValidationError::OutputMismatch { job: job.id() });
    }
    Ok(())
}

pub(crate) fn validate_loaded_crafting_job(
    registries: &Registries,
    job: &ProductionJobRecord,
) -> Result<(), CraftingJobValidationError> {
    let (definition, required_duration) =
        if let Some(definition) = registries.crafting().get_manual(job.process()) {
            (definition, None)
        } else if let Some(powered) = registries.crafting().get_powered(job.process()) {
            let transform = registries
                .crafting()
                .get_manual(powered.transform())
                .unwrap_or_else(|| {
                    panic!(
                        "validated powered craft {} lost transform {}",
                        powered.process().value(),
                        powered.transform().value()
                    )
                });
            (
                transform,
                Some(validate_powered_craft_resources(registries, powered, job)?),
            )
        } else {
            return Ok(());
        };
    let batch = validate_manual_craft_batch(definition, job.consumed_mass(), job.consumed_inputs())
        .map_err(|error| match error {
            ManualCraftBatchError::EmptyInput => {
                CraftingJobValidationError::EmptyInput { job: job.id() }
            }
            ManualCraftBatchError::InputCommodityMismatch => {
                CraftingJobValidationError::InputCommodityMismatch { job: job.id() }
            }
            ManualCraftBatchError::InputCompositionMismatch => {
                CraftingJobValidationError::InputCompositionMismatch { job: job.id() }
            }
            ManualCraftBatchError::MixedInputTemperature => {
                CraftingJobValidationError::MixedInputTemperature { job: job.id() }
            }
            ManualCraftBatchError::InputMassNotWholeBatches {
                consumed,
                batch_mass,
            } => CraftingJobValidationError::InputMassNotWholeBatches {
                job: job.id(),
                consumed,
                batch_mass,
            },
        })?;
    let batches = batch.batches();
    let required_duration = match required_duration {
        Some(duration) => duration,
        None => match validate_manual_craft_resources(registries, definition, job)? {
            Some(duration) => duration,
            None => resolve_manual_craft_hand_duration(definition.duration(), batches).ok_or(
                CraftingJobValidationError::DurationOverflow {
                    job: job.id(),
                    batches,
                },
            )?,
        },
    };
    if job.active_duration() != required_duration {
        return Err(CraftingJobValidationError::DurationMismatch {
            job: job.id(),
            stored: job.active_duration(),
            required: required_duration,
        });
    }
    let temperature = batch.temperature();
    let expected_outputs = reconstruct_manual_craft_outputs(definition, job, batches, temperature)?;
    validate_manual_craft_outputs(job, &expected_outputs)
}
