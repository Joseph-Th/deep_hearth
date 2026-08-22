//! Exhaustive replay validation for persisted constituent-separation production jobs.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::capability::CapabilityValue;
use crate::core::quantity::{Energy, Mass};
use crate::energy::{
    EnergyCarrier, PowerDurationError, calculate_mass_specific_energy,
    calculate_power_duration_ceiling,
};
use crate::equipment::resolve_equipment_capability;
use crate::maintenance::{ActiveConditionDurationError, Condition};
use crate::production::{ProductionJobId, ProductionJobRecord};
use crate::registry::Registries;

use super::{ConstituentSeparationBatchError, resolve_separation_outputs};
use crate::ore_processing::timing::OreProcessActiveTiming;
use crate::ore_processing::{
    ConstituentSeparationProcessDefinition, MassFlowDurationError,
    calculate_mass_flow_duration_ceiling,
};

/// Persistent-state failure found while replaying an in-flight constituent-separation job.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConstituentSeparationJobValidationError {
    MissingEnergy {
        job: ProductionJobId,
    },
    UnexpectedReleasedEnergy {
        job: ProductionJobId,
    },
    MissingEquipmentProvider {
        job: ProductionJobId,
    },
    UnknownEquipmentDefinition {
        job: ProductionJobId,
    },
    UnknownEnergyDefinition {
        job: ProductionJobId,
    },
    MissingMassFlowCapability {
        job: ProductionJobId,
    },
    MissingMaximumBatchMassCapability {
        job: ProductionJobId,
    },
    BatchMassExceeded {
        job: ProductionJobId,
        selected: Mass,
        maximum: Mass,
    },
    Batch {
        job: ProductionJobId,
        error: ConstituentSeparationBatchError,
    },
    WrongEnergyCarrier {
        job: ProductionJobId,
        required: EnergyCarrier,
        provided: EnergyCarrier,
    },
    EnergyMismatch {
        job: ProductionJobId,
        traced: Energy,
        required: Energy,
    },
    ThroughputDuration {
        job: ProductionJobId,
        error: MassFlowDurationError,
    },
    EnergyDuration {
        job: ProductionJobId,
        error: PowerDurationError,
    },
    ConditionDuration {
        job: ProductionJobId,
        error: ActiveConditionDurationError,
    },
    DurationMismatch {
        job: ProductionJobId,
        stored_ticks: u64,
        required_ticks: u64,
    },
    MissingConditionOutcome {
        job: ProductionJobId,
    },
    ConditionOutcomeMismatch {
        job: ProductionJobId,
        stored: Condition,
        required: Condition,
    },
    OutputMismatch {
        job: ProductionJobId,
    },
}

impl Display for ConstituentSeparationJobValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingEnergy { job } => write!(
                formatter,
                "constituent-separation job {} is missing consumed energy",
                job.value()
            ),
            Self::UnexpectedReleasedEnergy { job } => write!(
                formatter,
                "constituent-separation job {} unexpectedly stores released energy",
                job.value()
            ),
            Self::MissingEquipmentProvider { job } => write!(
                formatter,
                "constituent-separation job {} is missing its equipment trace",
                job.value()
            ),
            Self::UnknownEquipmentDefinition { job } => write!(
                formatter,
                "constituent-separation job {} references an unknown equipment definition",
                job.value()
            ),
            Self::UnknownEnergyDefinition { job } => write!(
                formatter,
                "constituent-separation job {} references an unknown energy-store definition",
                job.value()
            ),
            Self::MissingMassFlowCapability { job } => write!(
                formatter,
                "constituent-separation job {} equipment trace has no usable mass-flow capability",
                job.value()
            ),
            Self::MissingMaximumBatchMassCapability { job } => write!(
                formatter,
                "constituent-separation job {} equipment trace has no usable maximum-batch capability",
                job.value()
            ),
            Self::BatchMassExceeded {
                job,
                selected,
                maximum,
            } => write!(
                formatter,
                "constituent-separation job {} selected {} mg above its traced {} mg batch limit",
                job.value(),
                selected.milligrams(),
                maximum.milligrams()
            ),
            Self::Batch { job, error } => write!(
                formatter,
                "constituent-separation job {} input replay failed: {error}",
                job.value()
            ),
            Self::WrongEnergyCarrier {
                job,
                required,
                provided,
            } => write!(
                formatter,
                "constituent-separation job {} requires {required:?} energy but stores {provided:?}",
                job.value()
            ),
            Self::EnergyMismatch {
                job,
                traced,
                required,
            } => write!(
                formatter,
                "constituent-separation job {} stores {} nJ but replay requires {} nJ",
                job.value(),
                traced.nanojoules(),
                required.nanojoules()
            ),
            Self::ThroughputDuration { job, error } => write!(
                formatter,
                "constituent-separation job {} throughput replay failed: {error}",
                job.value()
            ),
            Self::EnergyDuration { job, error } => write!(
                formatter,
                "constituent-separation job {} energy-duration replay failed: {error}",
                job.value()
            ),
            Self::ConditionDuration { job, error } => write!(
                formatter,
                "constituent-separation job {} condition replay failed: {error}",
                job.value()
            ),
            Self::DurationMismatch {
                job,
                stored_ticks,
                required_ticks,
            } => write!(
                formatter,
                "constituent-separation job {} stores {stored_ticks} active ticks but replay requires {required_ticks}",
                job.value()
            ),
            Self::MissingConditionOutcome { job } => write!(
                formatter,
                "constituent-separation job {} is missing its equipment-condition outcome",
                job.value()
            ),
            Self::ConditionOutcomeMismatch {
                job,
                stored,
                required,
            } => write!(
                formatter,
                "constituent-separation job {} stores condition {} ppm but replay requires {} ppm",
                job.value(),
                stored.parts_per_million(),
                required.parts_per_million()
            ),
            Self::OutputMismatch { job } => write!(
                formatter,
                "constituent-separation job {} output streams do not match composition replay",
                job.value()
            ),
        }
    }
}

impl Error for ConstituentSeparationJobValidationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Batch { error, .. } => Some(error),
            Self::ThroughputDuration { error, .. } => Some(error),
            Self::EnergyDuration { error, .. } => Some(error),
            Self::ConditionDuration { error, .. } => Some(error),
            Self::MissingEnergy { .. }
            | Self::UnexpectedReleasedEnergy { .. }
            | Self::MissingEquipmentProvider { .. }
            | Self::UnknownEquipmentDefinition { .. }
            | Self::UnknownEnergyDefinition { .. }
            | Self::MissingMassFlowCapability { .. }
            | Self::MissingMaximumBatchMassCapability { .. }
            | Self::BatchMassExceeded { .. }
            | Self::WrongEnergyCarrier { .. }
            | Self::EnergyMismatch { .. }
            | Self::DurationMismatch { .. }
            | Self::MissingConditionOutcome { .. }
            | Self::ConditionOutcomeMismatch { .. }
            | Self::OutputMismatch { .. } => None,
        }
    }
}

pub(crate) fn validate_loaded_constituent_separation_job(
    registries: &Registries,
    job: &ProductionJobRecord,
) -> Result<(), ConstituentSeparationJobValidationError> {
    let Some(definition) = registries
        .ore_processing()
        .get_constituent_separation(job.process())
    else {
        return Ok(());
    };
    let consumed_energy = job
        .consumed_energy()
        .ok_or(ConstituentSeparationJobValidationError::MissingEnergy { job: job.id() })?;
    if job.released_energy().is_some() {
        return Err(
            ConstituentSeparationJobValidationError::UnexpectedReleasedEnergy { job: job.id() },
        );
    }
    let provider = job.equipment_provider().ok_or(
        ConstituentSeparationJobValidationError::MissingEquipmentProvider { job: job.id() },
    )?;
    let equipment_definition = registries
        .equipment()
        .get_equipment(provider.definition())
        .ok_or(
            ConstituentSeparationJobValidationError::UnknownEquipmentDefinition { job: job.id() },
        )?;
    let energy_definition = registries
        .energy()
        .get_store(consumed_energy.definition())
        .ok_or(
            ConstituentSeparationJobValidationError::UnknownEnergyDefinition { job: job.id() },
        )?;
    let processing_rate = match resolve_equipment_capability(
        equipment_definition,
        provider.condition(),
        definition.mass_flow_capability(),
    ) {
        Some(CapabilityValue::MassFlow(rate)) => rate,
        Some(_) | None => {
            return Err(
                ConstituentSeparationJobValidationError::MissingMassFlowCapability {
                    job: job.id(),
                },
            );
        }
    };
    let maximum_batch_mass = match resolve_equipment_capability(
        equipment_definition,
        provider.condition(),
        definition.max_batch_mass_capability(),
    ) {
        Some(CapabilityValue::Mass(mass)) => mass,
        Some(_) | None => {
            return Err(
                ConstituentSeparationJobValidationError::MissingMaximumBatchMassCapability {
                    job: job.id(),
                },
            );
        }
    };
    if job.consumed_mass() > maximum_batch_mass {
        return Err(ConstituentSeparationJobValidationError::BatchMassExceeded {
            job: job.id(),
            selected: job.consumed_mass(),
            maximum: maximum_batch_mass,
        });
    }
    let expected =
        resolve_separation_outputs(definition, job.consumed_inputs()).map_err(|error| {
            ConstituentSeparationJobValidationError::Batch {
                job: job.id(),
                error,
            }
        })?;
    if job.output_streams().len() != 2 {
        return Err(ConstituentSeparationJobValidationError::OutputMismatch { job: job.id() });
    }
    for (stream_id, outputs) in [
        (
            ConstituentSeparationProcessDefinition::TARGET_STREAM,
            expected.target.as_slice(),
        ),
        (
            ConstituentSeparationProcessDefinition::RESIDUE_STREAM,
            expected.residue.as_slice(),
        ),
    ] {
        let Some(stored) = job
            .output_streams()
            .iter()
            .find(|stream| stream.id() == stream_id)
        else {
            return Err(ConstituentSeparationJobValidationError::OutputMismatch { job: job.id() });
        };
        if stored.outputs() != outputs {
            return Err(ConstituentSeparationJobValidationError::OutputMismatch { job: job.id() });
        }
    }
    if consumed_energy.carrier() != definition.energy_carrier() {
        return Err(
            ConstituentSeparationJobValidationError::WrongEnergyCarrier {
                job: job.id(),
                required: definition.energy_carrier(),
                provided: consumed_energy.carrier(),
            },
        );
    }
    let required_energy =
        calculate_mass_specific_energy(job.consumed_mass(), definition.specific_energy());
    if consumed_energy.energy() != required_energy {
        return Err(ConstituentSeparationJobValidationError::EnergyMismatch {
            job: job.id(),
            traced: consumed_energy.energy(),
            required: required_energy,
        });
    }
    let throughput_duration = calculate_mass_flow_duration_ceiling(
        processing_rate,
        job.consumed_mass(),
        registries.core().physical_tick_duration(),
    )
    .map_err(
        |error| ConstituentSeparationJobValidationError::ThroughputDuration {
            job: job.id(),
            error,
        },
    )?;
    let energy_duration = calculate_power_duration_ceiling(
        energy_definition.max_output_power(),
        required_energy,
        registries.core().physical_tick_duration(),
    )
    .map_err(
        |error| ConstituentSeparationJobValidationError::EnergyDuration {
            job: job.id(),
            error,
        },
    )?;
    let timing = OreProcessActiveTiming::new(throughput_duration, energy_duration);
    let required_duration = timing.duration();
    if job.active_duration() != required_duration {
        return Err(ConstituentSeparationJobValidationError::DurationMismatch {
            job: job.id(),
            stored_ticks: job.active_duration().value(),
            required_ticks: required_duration.value(),
        });
    }
    let required_condition = timing
        .condition_after(
            definition.condition_wear_ppm_per_active_tick(),
            provider.condition(),
        )
        .map_err(
            |error| ConstituentSeparationJobValidationError::ConditionDuration {
                job: job.id(),
                error,
            },
        )?;
    let stored_condition = job.equipment_condition_after().ok_or(
        ConstituentSeparationJobValidationError::MissingConditionOutcome { job: job.id() },
    )?;
    if stored_condition != required_condition {
        return Err(
            ConstituentSeparationJobValidationError::ConditionOutcomeMismatch {
                job: job.id(),
                stored: stored_condition,
                required: required_condition,
            },
        );
    }
    Ok(())
}
