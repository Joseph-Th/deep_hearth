//! Persistence replay validation for thermal production jobs using the same physical derivations as runtime.

use crate::core::quantity::{Energy, Power, Temperature};
use crate::equipment::EquipmentDefinition;
use crate::maintenance::Condition;
use crate::production::{ProductionJobId, ProductionJobRecord};
use crate::registry::Registries;

use super::super::casting_execution::validate_loaded_casting_job;
use super::super::equipment_physics::{
    ThermalBatchLimitError, ThermalPowerTemperatureError, ThermalPowerTemperatureLimits,
    ThermalTransferTimingError, resolve_thermal_power_temperature_limits,
    resolve_thermal_transfer_timing, validate_thermal_batch_mass,
};
use super::super::melting_execution::validate_loaded_melting_job;
use super::registry::SensibleHeatingProcessDefinition;
use super::sensible_batch::{
    ResolvedSensibleHeatingBatch, SensibleHeatingBatchError, resolve_sensible_heating_batch,
};

mod error;

pub use error::ThermalJobValidationError;

struct SensibleHeatingReplayContext<'registry> {
    definition: SensibleHeatingProcessDefinition,
    equipment: &'registry EquipmentDefinition,
    condition_before: Condition,
    traced_energy: Energy,
    external_power: Power,
}

fn resolve_sensible_heating_resources<'registry>(
    registries: &'registry Registries,
    job: &ProductionJobRecord,
    definition: SensibleHeatingProcessDefinition,
) -> Result<SensibleHeatingReplayContext<'registry>, ThermalJobValidationError> {
    let consumed_energy = job
        .consumed_energy()
        .ok_or(ThermalJobValidationError::MissingEnergy { job: job.id() })?;
    let provider = job
        .equipment_provider()
        .ok_or(ThermalJobValidationError::MissingEquipmentProvider { job: job.id() })?;
    let equipment = registries
        .equipment()
        .get_equipment(provider.definition())
        .ok_or(ThermalJobValidationError::UnknownEquipmentDefinition { job: job.id() })?;
    let energy_definition = registries
        .energy()
        .get_store(consumed_energy.definition())
        .ok_or(ThermalJobValidationError::MissingEnergy { job: job.id() })?;
    if consumed_energy.carrier() != definition.energy_carrier() {
        return Err(ThermalJobValidationError::WrongEnergyCarrier {
            job: job.id(),
            required: definition.energy_carrier(),
            provided: consumed_energy.carrier(),
        });
    }
    Ok(SensibleHeatingReplayContext {
        definition,
        equipment,
        condition_before: provider.condition(),
        traced_energy: consumed_energy.energy(),
        external_power: energy_definition.max_output_power(),
    })
}

fn resolve_sensible_heating_target(
    job: &ProductionJobRecord,
) -> Result<Temperature, ThermalJobValidationError> {
    let output_stream = job
        .single_output_stream()
        .ok_or(ThermalJobValidationError::OutputMismatch { job: job.id() })?;
    let target = output_stream
        .outputs()
        .first()
        .ok_or(ThermalJobValidationError::OutputMismatch { job: job.id() })?
        .temperature();
    if output_stream
        .outputs()
        .iter()
        .any(|output| output.temperature() != target)
    {
        return Err(ThermalJobValidationError::MixedOutputTemperatures { job: job.id() });
    }
    Ok(target)
}

fn validate_sensible_heating_equipment(
    job: &ProductionJobRecord,
    context: &SensibleHeatingReplayContext<'_>,
    target: Temperature,
) -> Result<ThermalPowerTemperatureLimits, ThermalJobValidationError> {
    let limits = resolve_thermal_power_temperature_limits(
        context.equipment,
        context.condition_before,
        context.definition.heating_power_capability(),
        context.definition.max_temperature_capability(),
    )
    .map_err(|error| match error {
        ThermalPowerTemperatureError::MissingTransferPower => {
            ThermalJobValidationError::MissingHeatingPowerCapability { job: job.id() }
        }
        ThermalPowerTemperatureError::MissingMaximumTemperature => {
            ThermalJobValidationError::MissingMaximumTemperatureCapability { job: job.id() }
        }
    })?;
    let maximum_temperature = limits.maximum_temperature();
    if target > maximum_temperature {
        return Err(ThermalJobValidationError::TargetExceedsEquipmentMaximum {
            job: job.id(),
            target,
            maximum: maximum_temperature,
        });
    }
    validate_thermal_batch_mass(
        context.equipment,
        context.condition_before,
        context.definition.max_batch_mass_capability(),
        job.consumed_mass(),
    )
    .map_err(|error| match error {
        ThermalBatchLimitError::MissingMaximumBatchMass => {
            ThermalJobValidationError::MissingMaximumBatchMassCapability { job: job.id() }
        }
        ThermalBatchLimitError::BatchMassExceeded { selected, maximum } => {
            ThermalJobValidationError::BatchMassExceedsEquipmentCapacity {
                job: job.id(),
                selected,
                maximum,
            }
        }
    })?;
    Ok(limits)
}

fn resolve_sensible_heating_batch_replay(
    registries: &Registries,
    job: &ProductionJobRecord,
    target: Temperature,
    traced_energy: Energy,
) -> Result<ResolvedSensibleHeatingBatch, ThermalJobValidationError> {
    let batch =
        resolve_sensible_heating_batch(registries.materials(), job.consumed_inputs(), target)
            .map_err(|error| map_sensible_heating_batch_error(job.id(), error))?;
    let required_energy = batch.required_energy();
    if traced_energy != required_energy {
        return Err(ThermalJobValidationError::EnergyMismatch {
            job: job.id(),
            traced: traced_energy,
            required: required_energy,
        });
    }
    Ok(batch)
}

fn validate_sensible_heating_timing(
    registries: &Registries,
    job: &ProductionJobRecord,
    context: &SensibleHeatingReplayContext<'_>,
    limits: ThermalPowerTemperatureLimits,
    required_energy: Energy,
) -> Result<(), ThermalJobValidationError> {
    let timing = resolve_thermal_transfer_timing(
        registries,
        limits.transfer_power(),
        context.external_power,
        required_energy,
        context.definition.condition_wear_ppm_per_active_tick(),
        context.condition_before,
    )
    .map_err(|error| match error {
        ThermalTransferTimingError::Duration(error) => ThermalJobValidationError::Duration {
            job: job.id(),
            error,
        },
        ThermalTransferTimingError::ConditionDuration(error) => {
            ThermalJobValidationError::ConditionDuration {
                job: job.id(),
                error,
            }
        }
    })?;
    if job.active_duration() != timing.duration() {
        return Err(ThermalJobValidationError::DurationMismatch {
            job: job.id(),
            stored: job.active_duration(),
            required: timing.duration(),
        });
    }
    let stored_condition = job
        .equipment_condition_after()
        .ok_or(ThermalJobValidationError::MissingEquipmentConditionOutcome { job: job.id() })?;
    if stored_condition != timing.condition_after() {
        return Err(
            ThermalJobValidationError::EquipmentConditionOutcomeMismatch {
                job: job.id(),
                stored: stored_condition,
                required: timing.condition_after(),
            },
        );
    }
    Ok(())
}

fn validate_sensible_heating_outputs(
    job: &ProductionJobRecord,
    batch: ResolvedSensibleHeatingBatch,
) -> Result<(), ThermalJobValidationError> {
    let output_stream = job
        .single_output_stream()
        .ok_or(ThermalJobValidationError::OutputMismatch { job: job.id() })?;
    let mut expected_outputs = batch.into_outputs();
    expected_outputs.sort();
    let mut actual_outputs = output_stream.outputs().to_vec();
    actual_outputs.sort();
    if actual_outputs != expected_outputs {
        return Err(ThermalJobValidationError::OutputMismatch { job: job.id() });
    }
    Ok(())
}

fn validate_loaded_sensible_heating_job(
    registries: &Registries,
    job: &ProductionJobRecord,
    definition: SensibleHeatingProcessDefinition,
) -> Result<(), ThermalJobValidationError> {
    let context = resolve_sensible_heating_resources(registries, job, definition)?;
    let target = resolve_sensible_heating_target(job)?;
    let limits = validate_sensible_heating_equipment(job, &context, target)?;
    let batch =
        resolve_sensible_heating_batch_replay(registries, job, target, context.traced_energy)?;
    validate_sensible_heating_timing(registries, job, &context, limits, batch.required_energy())?;
    validate_sensible_heating_outputs(job, batch)
}

fn map_sensible_heating_batch_error(
    job: ProductionJobId,
    error: SensibleHeatingBatchError,
) -> ThermalJobValidationError {
    match error {
        SensibleHeatingBatchError::TargetBelowInputTemperature { current, target } => {
            ThermalJobValidationError::TargetBelowInputTemperature {
                job,
                current,
                target,
            }
        }
        SensibleHeatingBatchError::Heat(error) => ThermalJobValidationError::Heat { job, error },
        SensibleHeatingBatchError::ArithmeticOverflow => {
            ThermalJobValidationError::RequiredEnergyOverflow { job }
        }
        SensibleHeatingBatchError::Output(error) => {
            ThermalJobValidationError::OutputConstruction { job, error }
        }
    }
}

/// Recomputes the physical contract of an in-flight thermal job from persisted input traces.
///
/// Operation-specific validators use the same pure physical derivation used during runtime
/// resolution so save tampering cannot silently alter required energy, duration, wear, or output.
pub(crate) fn validate_loaded_thermal_job(
    registries: &Registries,
    job: &ProductionJobRecord,
) -> Result<(), ThermalJobValidationError> {
    if let Some(definition) = registries.thermal().get_casting(job.process()) {
        return validate_loaded_casting_job(registries, job, definition)
            .map_err(ThermalJobValidationError::Casting);
    }
    if let Some(definition) = registries.thermal().get_melting(job.process()) {
        return validate_loaded_melting_job(registries, job, definition)
            .map_err(ThermalJobValidationError::Melting);
    }
    match registries.thermal().get_sensible_heating(job.process()) {
        Some(definition) => validate_loaded_sensible_heating_job(registries, job, definition),
        None => Ok(()),
    }
}
