//! Persistence replay validation for thermal production jobs using the same physical derivations as runtime.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Energy, Mass, Temperature};
use crate::core::time::TickSpan;
use crate::energy::{EnergyCarrier, PowerDurationError};
use crate::maintenance::{ActiveConditionDurationError, Condition};
use crate::material::MaterialLotSpecError;
use crate::production::{ProductionJobId, ProductionJobRecord};
use crate::registry::Registries;

use super::super::PhaseSensibleHeatError;
use super::super::casting_execution::{CastingJobValidationError, validate_loaded_casting_job};
use super::super::equipment_physics::{
    ThermalBatchLimitError, ThermalPowerTemperatureError, ThermalTransferTimingError,
    resolve_thermal_power_temperature_limits, resolve_thermal_transfer_timing,
    validate_thermal_batch_mass,
};
use super::super::melting_execution::{MeltingJobValidationError, validate_loaded_melting_job};
use super::sensible_batch::{SensibleHeatingBatchError, resolve_sensible_heating_batch};

/// Invalid persisted operation-specific thermal semantics discovered during exhaustive load audit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ThermalJobValidationError {
    Casting(CastingJobValidationError),
    Melting(MeltingJobValidationError),
    MissingEquipmentProvider {
        job: ProductionJobId,
    },
    UnknownEquipmentDefinition {
        job: ProductionJobId,
    },
    MissingHeatingPowerCapability {
        job: ProductionJobId,
    },
    MissingMaximumTemperatureCapability {
        job: ProductionJobId,
    },
    MissingMaximumBatchMassCapability {
        job: ProductionJobId,
    },
    TargetExceedsEquipmentMaximum {
        job: ProductionJobId,
        target: Temperature,
        maximum: Temperature,
    },
    BatchMassExceedsEquipmentCapacity {
        job: ProductionJobId,
        selected: Mass,
        maximum: Mass,
    },
    MissingEnergy {
        job: ProductionJobId,
    },
    WrongEnergyCarrier {
        job: ProductionJobId,
        required: EnergyCarrier,
        provided: EnergyCarrier,
    },
    MixedOutputTemperatures {
        job: ProductionJobId,
    },
    TargetBelowInputTemperature {
        job: ProductionJobId,
        current: Temperature,
        target: Temperature,
    },
    Heat {
        job: ProductionJobId,
        error: PhaseSensibleHeatError,
    },
    RequiredEnergyOverflow {
        job: ProductionJobId,
    },
    EnergyMismatch {
        job: ProductionJobId,
        traced: Energy,
        required: Energy,
    },
    OutputConstruction {
        job: ProductionJobId,
        error: MaterialLotSpecError,
    },
    OutputMismatch {
        job: ProductionJobId,
    },
    Duration {
        job: ProductionJobId,
        error: PowerDurationError,
    },
    ConditionDuration {
        job: ProductionJobId,
        error: ActiveConditionDurationError,
    },
    DurationMismatch {
        job: ProductionJobId,
        stored: TickSpan,
        required: TickSpan,
    },
    MissingEquipmentConditionOutcome {
        job: ProductionJobId,
    },
    EquipmentConditionOutcomeMismatch {
        job: ProductionJobId,
        stored: Condition,
        required: Condition,
    },
}

impl Display for ThermalJobValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Casting(error) => write!(formatter, "invalid casting job: {error}"),
            Self::Melting(error) => write!(formatter, "invalid melting job: {error}"),
            Self::MissingEquipmentProvider { job } => write!(
                formatter,
                "sensible-heating job {} has no equipment provider trace",
                job.value()
            ),
            Self::UnknownEquipmentDefinition { job } => write!(
                formatter,
                "sensible-heating job {} references an unavailable equipment definition",
                job.value()
            ),
            Self::MissingHeatingPowerCapability { job } => write!(
                formatter,
                "sensible-heating job {} provider lacks configured heating-power capability",
                job.value()
            ),
            Self::MissingMaximumTemperatureCapability { job } => write!(
                formatter,
                "sensible-heating job {} provider lacks configured maximum-temperature capability",
                job.value()
            ),
            Self::MissingMaximumBatchMassCapability { job } => write!(
                formatter,
                "sensible-heating job {} provider lacks configured maximum-batch-mass capability",
                job.value()
            ),
            Self::TargetExceedsEquipmentMaximum {
                job,
                target,
                maximum,
            } => write!(
                formatter,
                "sensible-heating job {} target {} mK exceeds provider maximum {} mK",
                job.value(),
                target.millikelvin(),
                maximum.millikelvin()
            ),
            Self::BatchMassExceedsEquipmentCapacity {
                job,
                selected,
                maximum,
            } => write!(
                formatter,
                "sensible-heating job {} batch {} mg exceeds provider capacity {} mg",
                job.value(),
                selected.milligrams(),
                maximum.milligrams()
            ),
            Self::MissingEnergy { job } => write!(
                formatter,
                "sensible-heating job {} has no consumed energy trace",
                job.value()
            ),
            Self::WrongEnergyCarrier {
                job,
                required,
                provided,
            } => write!(
                formatter,
                "sensible-heating job {} requires {required:?} energy but traces {provided:?}",
                job.value()
            ),
            Self::MixedOutputTemperatures { job } => write!(
                formatter,
                "sensible-heating job {} contains multiple committed target temperatures",
                job.value()
            ),
            Self::TargetBelowInputTemperature {
                job,
                current,
                target,
            } => write!(
                formatter,
                "sensible-heating job {} target {} mK is below consumed input temperature {} mK",
                job.value(),
                target.millikelvin(),
                current.millikelvin()
            ),
            Self::Heat { job, error } => write!(
                formatter,
                "sensible-heating job {} cannot reproduce sensible heat: {error}",
                job.value()
            ),
            Self::RequiredEnergyOverflow { job } => write!(
                formatter,
                "sensible-heating job {} required energy overflows authoritative storage",
                job.value()
            ),
            Self::EnergyMismatch {
                job,
                traced,
                required,
            } => write!(
                formatter,
                "sensible-heating job {} traces {} nJ but consumed matter requires {} nJ",
                job.value(),
                traced.nanojoules(),
                required.nanojoules()
            ),
            Self::OutputConstruction { job, error } => write!(
                formatter,
                "sensible-heating job {} cannot reconstruct output snapshot: {error}",
                job.value()
            ),
            Self::OutputMismatch { job } => write!(
                formatter,
                "sensible-heating job {} output snapshot does not preserve consumed mass/composition at one target temperature",
                job.value()
            ),
            Self::Duration { job, error } => write!(
                formatter,
                "sensible-heating job {} duration cannot be recomputed: {error}",
                job.value()
            ),
            Self::ConditionDuration { job, error } => write!(
                formatter,
                "sensible-heating job {} exceeds equipment condition lifetime: {error}",
                job.value()
            ),
            Self::DurationMismatch {
                job,
                stored,
                required,
            } => write!(
                formatter,
                "sensible-heating job {} stores duration {} ticks but physics requires {} ticks",
                job.value(),
                stored.value(),
                required.value()
            ),
            Self::MissingEquipmentConditionOutcome { job } => write!(
                formatter,
                "sensible-heating job {} has no post-operation equipment condition",
                job.value()
            ),
            Self::EquipmentConditionOutcomeMismatch {
                job,
                stored,
                required,
            } => write!(
                formatter,
                "sensible-heating job {} stores post-operation condition {} ppm but active-time wear requires {} ppm",
                job.value(),
                stored.parts_per_million(),
                required.parts_per_million()
            ),
        }
    }
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

impl Error for ThermalJobValidationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Casting(error) => Some(error),
            Self::Melting(error) => Some(error),
            Self::Heat { job: _job, error } => Some(error),
            Self::OutputConstruction { job: _job, error } => Some(error),
            Self::Duration { job: _job, error } => Some(error),
            Self::ConditionDuration { job: _job, error } => Some(error),
            Self::MissingEquipmentProvider { job: _job }
            | Self::UnknownEquipmentDefinition { job: _job }
            | Self::MissingHeatingPowerCapability { job: _job }
            | Self::MissingMaximumTemperatureCapability { job: _job }
            | Self::MissingMaximumBatchMassCapability { job: _job }
            | Self::MissingEnergy { job: _job }
            | Self::MixedOutputTemperatures { job: _job }
            | Self::RequiredEnergyOverflow { job: _job }
            | Self::OutputMismatch { job: _job }
            | Self::MissingEquipmentConditionOutcome { job: _job } => None,
            Self::TargetExceedsEquipmentMaximum {
                job: _job,
                target: _target,
                maximum: _maximum,
            } => None,
            Self::TargetBelowInputTemperature {
                job: _job,
                current: _current,
                target: _target,
            } => None,
            Self::BatchMassExceedsEquipmentCapacity {
                job: _job,
                selected: _selected,
                maximum: _maximum,
            } => None,
            Self::WrongEnergyCarrier {
                job: _job,
                required: _required,
                provided: _provided,
            } => None,
            Self::EnergyMismatch {
                job: _job,
                traced: _traced,
                required: _required,
            } => None,
            Self::DurationMismatch {
                job: _job,
                stored: _stored,
                required: _required,
            } => None,
            Self::EquipmentConditionOutcomeMismatch {
                job: _job,
                stored: _stored,
                required: _required,
            } => None,
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
    if registries
        .thermal()
        .get_sensible_heating(job.process())
        .is_none()
    {
        return Ok(());
    }
    let Some(consumed_energy) = job.consumed_energy() else {
        return Err(ThermalJobValidationError::MissingEnergy { job: job.id() });
    };
    let Some(provider) = job.equipment_provider() else {
        return Err(ThermalJobValidationError::MissingEquipmentProvider { job: job.id() });
    };
    let Some(equipment_definition) = registries.equipment().get_equipment(provider.definition())
    else {
        return Err(ThermalJobValidationError::UnknownEquipmentDefinition { job: job.id() });
    };
    let Some(energy_definition) = registries.energy().get_store(consumed_energy.definition())
    else {
        return Err(ThermalJobValidationError::MissingEnergy { job: job.id() });
    };
    let thermal_definition = match registries.thermal().get_sensible_heating(job.process()) {
        Some(definition) => definition,
        None => return Ok(()),
    };
    if consumed_energy.carrier() != thermal_definition.energy_carrier() {
        return Err(ThermalJobValidationError::WrongEnergyCarrier {
            job: job.id(),
            required: thermal_definition.energy_carrier(),
            provided: consumed_energy.carrier(),
        });
    }
    let Some(output_stream) = job.single_output_stream() else {
        return Err(ThermalJobValidationError::OutputMismatch { job: job.id() });
    };
    let Some(first_output) = output_stream.outputs().first() else {
        return Err(ThermalJobValidationError::OutputMismatch { job: job.id() });
    };
    let target = first_output.temperature();
    if output_stream
        .outputs()
        .iter()
        .any(|output| output.temperature() != target)
    {
        return Err(ThermalJobValidationError::MixedOutputTemperatures { job: job.id() });
    }
    let limits = resolve_thermal_power_temperature_limits(
        equipment_definition,
        provider.condition(),
        thermal_definition.heating_power_capability(),
        thermal_definition.max_temperature_capability(),
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
        equipment_definition,
        provider.condition(),
        thermal_definition.max_batch_mass_capability(),
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

    let batch =
        resolve_sensible_heating_batch(registries.materials(), job.consumed_inputs(), target)
            .map_err(|error| map_sensible_heating_batch_error(job.id(), error))?;
    let required_energy = batch.required_energy();
    if consumed_energy.energy() != required_energy {
        return Err(ThermalJobValidationError::EnergyMismatch {
            job: job.id(),
            traced: consumed_energy.energy(),
            required: required_energy,
        });
    }
    let timing = resolve_thermal_transfer_timing(
        registries,
        limits.transfer_power(),
        energy_definition.max_output_power(),
        required_energy,
        thermal_definition.condition_wear_ppm_per_active_tick(),
        provider.condition(),
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
    let required_duration = timing.duration();
    let stored_duration = job.active_duration();
    if stored_duration != required_duration {
        return Err(ThermalJobValidationError::DurationMismatch {
            job: job.id(),
            stored: stored_duration,
            required: required_duration,
        });
    }
    let required_condition_after = timing.condition_after();
    let Some(stored_condition_after) = job.equipment_condition_after() else {
        return Err(ThermalJobValidationError::MissingEquipmentConditionOutcome { job: job.id() });
    };
    if stored_condition_after != required_condition_after {
        return Err(
            ThermalJobValidationError::EquipmentConditionOutcomeMismatch {
                job: job.id(),
                stored: stored_condition_after,
                required: required_condition_after,
            },
        );
    }

    let mut expected_outputs = batch.into_outputs();
    expected_outputs.sort();
    let mut actual_outputs = output_stream.outputs().to_vec();
    actual_outputs.sort();
    if actual_outputs != expected_outputs {
        return Err(ThermalJobValidationError::OutputMismatch { job: job.id() });
    }
    Ok(())
}
