//! Persistence replay validation for thermal production jobs using the same physical derivations as runtime.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::capability::CapabilityValue;
use crate::core::quantity::{Energy, Mass, Temperature};
use crate::core::time::TickSpan;
use crate::energy::{EnergyCarrier, PowerDurationError, calculate_power_duration_ceiling};
use crate::equipment::resolve_equipment_capability;
use crate::maintenance::{Condition, calculate_condition_after_active_ticks};
use crate::material::{MaterialLotSpec, MaterialLotSpecError};
use crate::production::{ProductionJobId, ProductionJobRecord};
use crate::registry::Registries;

use super::super::casting_execution::{CastingJobValidationError, validate_loaded_casting_job};
use super::super::melting_execution::{MeltingJobValidationError, validate_loaded_melting_job};
use super::super::{PhaseSensibleHeatError, calculate_phase_sensible_heat};

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

impl Error for ThermalJobValidationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Casting(error) => Some(error),
            Self::Melting(error) => Some(error),
            Self::Heat { job: _job, error } => Some(error),
            Self::OutputConstruction { job: _job, error } => Some(error),
            Self::Duration { job: _job, error } => Some(error),
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
    let heating_power = match resolve_equipment_capability(
        equipment_definition,
        provider.condition(),
        thermal_definition.heating_power_capability(),
    ) {
        Some(CapabilityValue::Power(power)) => power,
        Some(_) | None => {
            return Err(ThermalJobValidationError::MissingHeatingPowerCapability { job: job.id() });
        }
    };
    let maximum_temperature = match resolve_equipment_capability(
        equipment_definition,
        provider.condition(),
        thermal_definition.max_temperature_capability(),
    ) {
        Some(CapabilityValue::Temperature(temperature)) => temperature,
        Some(_) | None => {
            return Err(
                ThermalJobValidationError::MissingMaximumTemperatureCapability { job: job.id() },
            );
        }
    };
    if target > maximum_temperature {
        return Err(ThermalJobValidationError::TargetExceedsEquipmentMaximum {
            job: job.id(),
            target,
            maximum: maximum_temperature,
        });
    }
    let maximum_batch_mass = match resolve_equipment_capability(
        equipment_definition,
        provider.condition(),
        thermal_definition.max_batch_mass_capability(),
    ) {
        Some(CapabilityValue::Mass(mass)) => mass,
        Some(_) | None => {
            return Err(
                ThermalJobValidationError::MissingMaximumBatchMassCapability { job: job.id() },
            );
        }
    };
    if job.consumed_mass() > maximum_batch_mass {
        return Err(
            ThermalJobValidationError::BatchMassExceedsEquipmentCapacity {
                job: job.id(),
                selected: job.consumed_mass(),
                maximum: maximum_batch_mass,
            },
        );
    }

    let mut required_energy = Energy::ZERO;
    let mut output_masses = BTreeMap::new();
    for trace in job.consumed_inputs() {
        let profile = trace.profile();
        if target < profile.temperature() {
            return Err(ThermalJobValidationError::TargetBelowInputTemperature {
                job: job.id(),
                current: profile.temperature(),
                target,
            });
        }
        let heat = calculate_phase_sensible_heat(
            registries.materials(),
            trace.mass(),
            profile.commodity(),
            profile.composition(),
            profile.temperature(),
            target,
        )
        .map_err(|error| ThermalJobValidationError::Heat {
            job: job.id(),
            error,
        })?;
        required_energy = required_energy
            .checked_add(heat.energy())
            .ok_or(ThermalJobValidationError::RequiredEnergyOverflow { job: job.id() })?;
        let key = (
            profile.commodity(),
            profile.composition().clone(),
            profile.particle_size_distribution().cloned(),
        );
        let current = output_masses.get(&key).copied().unwrap_or(Mass::ZERO);
        output_masses.insert(
            key,
            current
                .checked_add(trace.mass())
                .ok_or(ThermalJobValidationError::RequiredEnergyOverflow { job: job.id() })?,
        );
    }
    if consumed_energy.energy() != required_energy {
        return Err(ThermalJobValidationError::EnergyMismatch {
            job: job.id(),
            traced: consumed_energy.energy(),
            required: required_energy,
        });
    }
    let transfer_power = heating_power.min(energy_definition.max_output_power());
    let required_duration = calculate_power_duration_ceiling(
        transfer_power,
        required_energy,
        registries.core().physical_tick_duration(),
    )
    .map_err(|error| ThermalJobValidationError::Duration {
        job: job.id(),
        error,
    })?;
    let stored_duration = job.active_duration();
    if stored_duration != required_duration {
        return Err(ThermalJobValidationError::DurationMismatch {
            job: job.id(),
            stored: stored_duration,
            required: required_duration,
        });
    }
    let required_condition_after = calculate_condition_after_active_ticks(
        thermal_definition.condition_wear_ppm_per_active_tick(),
        provider.condition(),
        required_duration,
    );
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

    let mut expected_outputs = Vec::with_capacity(output_masses.len());
    for ((commodity, composition, particle_size), mass) in output_masses {
        let output = match particle_size {
            Some(particle_size) => MaterialLotSpec::with_composition_and_particle_size(
                commodity,
                mass,
                target,
                composition,
                particle_size,
            ),
            None => MaterialLotSpec::with_composition(commodity, mass, target, composition),
        }
        .map_err(|error| ThermalJobValidationError::OutputConstruction {
            job: job.id(),
            error,
        })?;
        expected_outputs.push(output);
    }
    expected_outputs.sort();
    let mut actual_outputs = output_stream.outputs().to_vec();
    actual_outputs.sort();
    if actual_outputs != expected_outputs {
        return Err(ThermalJobValidationError::OutputMismatch { job: job.id() });
    }
    Ok(())
}
