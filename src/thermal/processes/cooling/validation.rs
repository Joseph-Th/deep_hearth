//! Trusted-load replay of solid-cooling heat and timing derivations.

use crate::production::ProductionJobRecord;
use crate::registry::Registries;

use super::super::super::equipment_physics::{
    ThermalBatchLimitError, ThermalPowerTemperatureError, ThermalTransferTimingError,
    resolve_thermal_power_temperature_limits, resolve_thermal_transfer_timing,
    validate_thermal_batch_mass, validate_thermal_process_capabilities,
};
use super::super::registry::SensibleCoolingProcessDefinition;
use super::errors::SensibleCoolingJobValidationError as E;
use super::resolve_sensible_cooling_batch;

pub(in crate::thermal) fn validate_loaded_sensible_cooling_job(
    registries: &Registries,
    job: &ProductionJobRecord,
    definition: SensibleCoolingProcessDefinition,
) -> Result<(), E> {
    let id = job.id();
    if job.consumed_energy().is_some() {
        return Err(E::UnexpectedConsumedEnergy { job: id });
    }
    let released = job
        .released_energy()
        .ok_or(E::MissingReleasedEnergy { job: id })?;
    let provider = job
        .equipment_provider()
        .ok_or(E::MissingEquipmentProvider { job: id })?;
    let equipment = registries
        .equipment()
        .get_equipment(provider.definition())
        .ok_or(E::UnknownEquipmentDefinition { job: id })?;
    let energy_definition = registries
        .energy()
        .get_store(released.definition())
        .ok_or(E::UnknownEnergyDefinition { job: id })?;
    validate_thermal_process_capabilities(
        registries,
        job.process(),
        equipment,
        provider.condition(),
    )
    .map_err(|error| E::Capability { job: id, error })?;
    let limits = resolve_thermal_power_temperature_limits(
        equipment,
        provider.condition(),
        definition.cooling_power_capability(),
        definition.max_temperature_capability(),
    )
    .map_err(|error| match error {
        ThermalPowerTemperatureError::MissingTransferPower => E::MissingCoolingPower { job: id },
        ThermalPowerTemperatureError::MissingMaximumTemperature => {
            E::MissingMaximumTemperature { job: id }
        }
    })?;
    validate_thermal_batch_mass(
        equipment,
        provider.condition(),
        definition.max_batch_mass_capability(),
        job.consumed_mass(),
    )
    .map_err(|error| match error {
        ThermalBatchLimitError::MissingMaximumBatchMass => E::MissingMaximumBatchMass { job: id },
        ThermalBatchLimitError::BatchMassExceeded { selected, maximum } => {
            E::BatchMassExceedsEquipmentCapacity {
                job: id,
                selected,
                maximum,
            }
        }
    })?;
    let batch =
        resolve_sensible_cooling_batch(registries.materials(), definition, job.consumed_inputs())
            .map_err(|error| E::Batch { job: id, error })?;
    if batch.hottest_input > limits.maximum_temperature() {
        return Err(E::InputTemperatureExceedsEquipmentMaximum {
            job: id,
            input: batch.hottest_input,
            maximum: limits.maximum_temperature(),
        });
    }
    if released.carrier() != definition.energy_carrier() {
        return Err(E::WrongEnergyCarrier {
            job: id,
            required: definition.energy_carrier(),
            provided: released.carrier(),
        });
    }
    if released.energy() != batch.energy {
        return Err(E::EnergyMismatch {
            job: id,
            traced: released.energy(),
            required: batch.energy,
        });
    }
    let timing = resolve_thermal_transfer_timing(
        registries,
        limits.transfer_power(),
        energy_definition.max_input_power(),
        batch.energy,
        definition.condition_wear_ppm_per_active_tick(),
        provider.condition(),
    )
    .map_err(|error| match error {
        ThermalTransferTimingError::Duration(error) => E::Duration { job: id, error },
        ThermalTransferTimingError::ConditionDuration(error) => {
            E::ConditionDuration { job: id, error }
        }
    })?;
    if job.active_duration() != timing.duration() {
        return Err(E::DurationMismatch {
            job: id,
            stored: job.active_duration(),
            required: timing.duration(),
        });
    }
    let condition_after = job
        .equipment_condition_after()
        .ok_or(E::MissingEquipmentConditionOutcome { job: id })?;
    if condition_after != timing.condition_after() {
        return Err(E::EquipmentConditionMismatch {
            job: id,
            stored: condition_after,
            required: timing.condition_after(),
        });
    }
    let output = job
        .single_output_stream()
        .ok_or(E::OutputMismatch { job: id })?;
    let mut expected = batch.outputs;
    expected.sort();
    if output.outputs() != expected {
        return Err(E::OutputMismatch { job: id });
    }
    Ok(())
}
