//! Persisted melting replay validation against authored phase-change physics.

use crate::core::quantity::Power;
use crate::energy::ConsumedEnergyTrace;
use crate::equipment::EquipmentOperationTrace;
use crate::production::ProductionJobRecord;
use crate::registry::Registries;

use super::super::equipment_physics::{
    ThermalBatchLimitError, ThermalPowerTemperatureError, ThermalPowerTemperatureLimits,
    ThermalTransferTiming, ThermalTransferTimingError, resolve_thermal_power_temperature_limits,
    resolve_thermal_transfer_timing, validate_thermal_batch_mass,
};
use super::super::phase_change_batch::PurePhaseChangeBatch;
use super::{MeltingJobValidationError, MeltingProcessDefinition, resolve_melting_batch};

#[derive(Clone, Debug, PartialEq, Eq)]
struct MeltingReplayContext {
    consumed_energy: ConsumedEnergyTrace,
    provider: EquipmentOperationTrace,
    limits: ThermalPowerTemperatureLimits,
    source_max_output_power: Power,
}

fn resolve_melting_replay_context(
    registries: &Registries,
    job: &ProductionJobRecord,
    definition: &MeltingProcessDefinition,
) -> Result<MeltingReplayContext, MeltingJobValidationError> {
    let Some(consumed_energy) = job.consumed_energy() else {
        return Err(MeltingJobValidationError::MissingEnergy { job: job.id() });
    };
    let Some(provider) = job.equipment_provider() else {
        return Err(MeltingJobValidationError::MissingEquipmentProvider { job: job.id() });
    };
    let Some(equipment_definition) = registries.equipment().get_equipment(provider.definition())
    else {
        return Err(MeltingJobValidationError::UnknownEquipmentDefinition { job: job.id() });
    };
    let Some(energy_definition) = registries.energy().get_store(consumed_energy.definition())
    else {
        return Err(MeltingJobValidationError::UnknownEnergyDefinition { job: job.id() });
    };
    let limits = resolve_thermal_power_temperature_limits(
        equipment_definition,
        provider.condition(),
        definition.heating_power_capability(),
        definition.max_temperature_capability(),
    )
    .map_err(|error| match error {
        ThermalPowerTemperatureError::MissingTransferPower => {
            MeltingJobValidationError::MissingHeatingPowerCapability { job: job.id() }
        }
        ThermalPowerTemperatureError::MissingMaximumTemperature => {
            MeltingJobValidationError::MissingMaximumTemperatureCapability { job: job.id() }
        }
    })?;
    validate_thermal_batch_mass(
        equipment_definition,
        provider.condition(),
        definition.max_batch_mass_capability(),
        job.consumed_mass(),
    )
    .map_err(|error| match error {
        ThermalBatchLimitError::MissingMaximumBatchMass => {
            MeltingJobValidationError::MissingMaximumBatchMassCapability { job: job.id() }
        }
        ThermalBatchLimitError::BatchMassExceeded { selected, maximum } => {
            MeltingJobValidationError::BatchMassExceedsEquipmentCapacity {
                job: job.id(),
                selected,
                maximum,
            }
        }
    })?;
    Ok(MeltingReplayContext {
        consumed_energy,
        provider,
        limits,
        source_max_output_power: energy_definition.max_output_power(),
    })
}

fn resolve_loaded_melting_batch(
    registries: &Registries,
    job: &ProductionJobRecord,
    definition: &MeltingProcessDefinition,
    context: &MeltingReplayContext,
) -> Result<PurePhaseChangeBatch, MeltingJobValidationError> {
    let batch = resolve_melting_batch(registries.materials(), definition, job.consumed_inputs())
        .map_err(|error| MeltingJobValidationError::Batch {
            job: job.id(),
            error,
        })?;
    if batch.melting_point > context.limits.maximum_temperature() {
        return Err(
            MeltingJobValidationError::MeltingPointExceedsEquipmentMaximum {
                job: job.id(),
                melting_point: batch.melting_point,
                maximum: context.limits.maximum_temperature(),
            },
        );
    }
    if context.consumed_energy.carrier() != definition.energy_carrier() {
        return Err(MeltingJobValidationError::WrongEnergyCarrier {
            job: job.id(),
            required: definition.energy_carrier(),
            provided: context.consumed_energy.carrier(),
        });
    }
    if context.consumed_energy.energy() != batch.transfer_energy {
        return Err(MeltingJobValidationError::EnergyMismatch {
            job: job.id(),
            traced: context.consumed_energy.energy(),
            required: batch.transfer_energy,
        });
    }
    Ok(batch)
}

fn resolve_melting_replay_timing(
    registries: &Registries,
    job: &ProductionJobRecord,
    definition: &MeltingProcessDefinition,
    context: &MeltingReplayContext,
    batch: &PurePhaseChangeBatch,
) -> Result<ThermalTransferTiming, MeltingJobValidationError> {
    resolve_thermal_transfer_timing(
        registries,
        context.limits.transfer_power(),
        context.source_max_output_power,
        batch.transfer_energy,
        definition.condition_wear_ppm_per_active_tick(),
        context.provider.condition(),
    )
    .map_err(|error| match error {
        ThermalTransferTimingError::Duration(error) => MeltingJobValidationError::Duration {
            job: job.id(),
            error,
        },
        ThermalTransferTimingError::ConditionDuration(error) => {
            MeltingJobValidationError::ConditionDuration {
                job: job.id(),
                error,
            }
        }
    })
}

fn validate_melting_replay_outcome(
    job: &ProductionJobRecord,
    batch: &PurePhaseChangeBatch,
    timing: ThermalTransferTiming,
) -> Result<(), MeltingJobValidationError> {
    let required_duration = timing.duration();
    let stored_duration = job.active_duration();
    if stored_duration != required_duration {
        return Err(MeltingJobValidationError::DurationMismatch {
            job: job.id(),
            stored: stored_duration,
            required: required_duration,
        });
    }
    let required_condition_after = timing.condition_after();
    let Some(stored_condition_after) = job.equipment_condition_after() else {
        return Err(MeltingJobValidationError::MissingEquipmentConditionOutcome { job: job.id() });
    };
    if stored_condition_after != required_condition_after {
        return Err(
            MeltingJobValidationError::EquipmentConditionOutcomeMismatch {
                job: job.id(),
                stored: stored_condition_after,
                required: required_condition_after,
            },
        );
    }
    let Some(output_stream) = job.single_output_stream() else {
        return Err(MeltingJobValidationError::OutputMismatch { job: job.id() });
    };
    if output_stream.outputs() != [batch.output.clone()] {
        return Err(MeltingJobValidationError::OutputMismatch { job: job.id() });
    }
    Ok(())
}

pub(in crate::thermal) fn validate_loaded_melting_job(
    registries: &Registries,
    job: &ProductionJobRecord,
    definition: &MeltingProcessDefinition,
) -> Result<(), MeltingJobValidationError> {
    let context = resolve_melting_replay_context(registries, job, definition)?;
    let batch = resolve_loaded_melting_batch(registries, job, definition, &context)?;
    let timing = resolve_melting_replay_timing(registries, job, definition, &context, &batch)?;
    validate_melting_replay_outcome(job, &batch, timing)
}
