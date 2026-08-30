//! Persisted casting replay validation against authored phase-change physics.

use crate::core::quantity::Power;
use crate::energy::ReleasedEnergyTrace;
use crate::equipment::EquipmentOperationTrace;
use crate::production::ProductionJobRecord;
use crate::registry::Registries;

use super::super::equipment_physics::{
    ThermalBatchLimitError, ThermalPowerTemperatureError, ThermalPowerTemperatureLimits,
    ThermalTransferTiming, ThermalTransferTimingError, resolve_thermal_power_temperature_limits,
    resolve_thermal_transfer_timing, validate_thermal_batch_mass,
};
use super::super::phase_change_batch::PurePhaseChangeBatch;
use super::{CastingJobValidationError, CastingProcessDefinition, resolve_casting_batch};

#[derive(Clone, Debug, PartialEq, Eq)]
struct CastingReplayContext {
    released_energy: ReleasedEnergyTrace,
    provider: EquipmentOperationTrace,
    limits: ThermalPowerTemperatureLimits,
    sink_max_input_power: Power,
}

fn resolve_casting_replay_context(
    registries: &Registries,
    job: &ProductionJobRecord,
    definition: CastingProcessDefinition,
) -> Result<CastingReplayContext, CastingJobValidationError> {
    if job.consumed_energy().is_some() {
        return Err(CastingJobValidationError::UnexpectedConsumedEnergy { job: job.id() });
    }
    let Some(released_energy) = job.released_energy() else {
        return Err(CastingJobValidationError::MissingReleasedEnergy { job: job.id() });
    };
    let Some(provider) = job.equipment_provider() else {
        return Err(CastingJobValidationError::MissingEquipmentProvider { job: job.id() });
    };
    let Some(equipment_definition) = registries.equipment().get_equipment(provider.definition())
    else {
        return Err(CastingJobValidationError::UnknownEquipmentDefinition { job: job.id() });
    };
    let Some(energy_definition) = registries.energy().get_store(released_energy.definition())
    else {
        return Err(CastingJobValidationError::UnknownEnergyDefinition { job: job.id() });
    };
    let limits = resolve_thermal_power_temperature_limits(
        equipment_definition,
        provider.condition(),
        definition.cooling_power_capability(),
        definition.max_temperature_capability(),
    )
    .map_err(|error| match error {
        ThermalPowerTemperatureError::MissingTransferPower => {
            CastingJobValidationError::MissingCoolingPowerCapability { job: job.id() }
        }
        ThermalPowerTemperatureError::MissingMaximumTemperature => {
            CastingJobValidationError::MissingMaximumTemperatureCapability { job: job.id() }
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
            CastingJobValidationError::MissingMaximumBatchMassCapability { job: job.id() }
        }
        ThermalBatchLimitError::BatchMassExceeded { selected, maximum } => {
            CastingJobValidationError::BatchMassExceedsEquipmentCapacity {
                job: job.id(),
                selected,
                maximum,
            }
        }
    })?;
    Ok(CastingReplayContext {
        released_energy,
        provider,
        limits,
        sink_max_input_power: energy_definition.max_input_power(),
    })
}

fn resolve_loaded_casting_batch(
    registries: &Registries,
    job: &ProductionJobRecord,
    definition: CastingProcessDefinition,
    context: &CastingReplayContext,
) -> Result<PurePhaseChangeBatch, CastingJobValidationError> {
    let batch = resolve_casting_batch(
        registries.materials(),
        definition.material(),
        definition.liquid_form(),
        definition.solid_form(),
        definition.output_temperature(),
        job.consumed_inputs(),
    )
    .map_err(|error| CastingJobValidationError::Batch {
        job: job.id(),
        error,
    })?;
    if batch.hottest_input > context.limits.maximum_temperature() {
        return Err(
            CastingJobValidationError::InputTemperatureExceedsEquipmentMaximum {
                job: job.id(),
                input: batch.hottest_input,
                maximum: context.limits.maximum_temperature(),
            },
        );
    }
    if context.released_energy.carrier() != definition.energy_carrier() {
        return Err(CastingJobValidationError::WrongEnergyCarrier {
            job: job.id(),
            required: definition.energy_carrier(),
            provided: context.released_energy.carrier(),
        });
    }
    if context.released_energy.energy() != batch.transfer_energy {
        return Err(CastingJobValidationError::ReleasedEnergyMismatch {
            job: job.id(),
            traced: context.released_energy.energy(),
            required: batch.transfer_energy,
        });
    }
    Ok(batch)
}

fn resolve_casting_replay_timing(
    registries: &Registries,
    job: &ProductionJobRecord,
    definition: CastingProcessDefinition,
    context: &CastingReplayContext,
    batch: &PurePhaseChangeBatch,
) -> Result<ThermalTransferTiming, CastingJobValidationError> {
    resolve_thermal_transfer_timing(
        registries,
        context.limits.transfer_power(),
        context.sink_max_input_power,
        batch.transfer_energy,
        definition.condition_wear_ppm_per_active_tick(),
        context.provider.condition(),
    )
    .map_err(|error| match error {
        ThermalTransferTimingError::Duration(error) => CastingJobValidationError::Duration {
            job: job.id(),
            error,
        },
        ThermalTransferTimingError::ConditionDuration(error) => {
            CastingJobValidationError::ConditionDuration {
                job: job.id(),
                error,
            }
        }
    })
}

fn validate_casting_replay_outcome(
    job: &ProductionJobRecord,
    batch: &PurePhaseChangeBatch,
    timing: ThermalTransferTiming,
) -> Result<(), CastingJobValidationError> {
    let required_duration = timing.duration();
    let stored_duration = job.active_duration();
    if stored_duration != required_duration {
        return Err(CastingJobValidationError::DurationMismatch {
            job: job.id(),
            stored: stored_duration,
            required: required_duration,
        });
    }
    let required_condition_after = timing.condition_after();
    let Some(stored_condition_after) = job.equipment_condition_after() else {
        return Err(CastingJobValidationError::MissingEquipmentConditionOutcome { job: job.id() });
    };
    if stored_condition_after != required_condition_after {
        return Err(
            CastingJobValidationError::EquipmentConditionOutcomeMismatch {
                job: job.id(),
                stored: stored_condition_after,
                required: required_condition_after,
            },
        );
    }
    let Some(output_stream) = job.single_output_stream() else {
        return Err(CastingJobValidationError::OutputMismatch { job: job.id() });
    };
    if output_stream.outputs() != [batch.output.clone()] {
        return Err(CastingJobValidationError::OutputMismatch { job: job.id() });
    }
    Ok(())
}

pub(in crate::thermal) fn validate_loaded_casting_job(
    registries: &Registries,
    job: &ProductionJobRecord,
    definition: CastingProcessDefinition,
) -> Result<(), CastingJobValidationError> {
    let context = resolve_casting_replay_context(registries, job, definition)?;
    let batch = resolve_loaded_casting_batch(registries, job, definition, &context)?;
    let timing = resolve_casting_replay_timing(registries, job, definition, &context, &batch)?;
    validate_casting_replay_outcome(job, &batch, timing)
}
