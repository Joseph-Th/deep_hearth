//! Trusted-load validation failures for persisted casting jobs.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::capability::CapabilityEvaluationError;
use crate::core::quantity::{Energy, Mass, Temperature};
use crate::core::time::TickSpan;
use crate::energy::{EnergyCarrier, PowerDurationError};
use crate::maintenance::{ActiveConditionDurationError, Condition};
use crate::production::ProductionJobId;

use super::super::CastingBatchError;

/// Invalid persisted casting semantics discovered during exhaustive load validation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CastingJobValidationError {
    UnexpectedConsumedEnergy {
        job: ProductionJobId,
    },
    MissingReleasedEnergy {
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
    Capability {
        job: ProductionJobId,
        error: CapabilityEvaluationError,
    },
    MissingCoolingPowerCapability {
        job: ProductionJobId,
    },
    MissingMaximumTemperatureCapability {
        job: ProductionJobId,
    },
    MissingMaximumBatchMassCapability {
        job: ProductionJobId,
    },
    BatchMassExceedsEquipmentCapacity {
        job: ProductionJobId,
        selected: Mass,
        maximum: Mass,
    },
    Batch {
        job: ProductionJobId,
        error: CastingBatchError,
    },
    InputTemperatureExceedsEquipmentMaximum {
        job: ProductionJobId,
        input: Temperature,
        maximum: Temperature,
    },
    WrongEnergyCarrier {
        job: ProductionJobId,
        required: EnergyCarrier,
        provided: EnergyCarrier,
    },
    ReleasedEnergyMismatch {
        job: ProductionJobId,
        traced: Energy,
        required: Energy,
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
    OutputMismatch {
        job: ProductionJobId,
    },
}

impl Display for CastingJobValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnexpectedConsumedEnergy { job } => write!(
                formatter,
                "casting job {} unexpectedly consumes finite energy",
                job.value()
            ),
            Self::MissingReleasedEnergy { job } => write!(
                formatter,
                "casting job {} has no released-energy trace",
                job.value()
            ),
            Self::MissingEquipmentProvider { job } => write!(
                formatter,
                "casting job {} has no equipment provider",
                job.value()
            ),
            Self::UnknownEquipmentDefinition { job } => write!(
                formatter,
                "casting job {} references unavailable equipment",
                job.value()
            ),
            Self::UnknownEnergyDefinition { job } => write!(
                formatter,
                "casting job {} references unavailable thermal sink definition",
                job.value()
            ),
            Self::Capability { job, error } => write!(
                formatter,
                "casting job {} provider fails authored process capabilities: {error}",
                job.value()
            ),
            Self::MissingCoolingPowerCapability { job } => write!(
                formatter,
                "casting job {} provider lacks cooling power",
                job.value()
            ),
            Self::MissingMaximumTemperatureCapability { job } => write!(
                formatter,
                "casting job {} provider lacks maximum temperature",
                job.value()
            ),
            Self::MissingMaximumBatchMassCapability { job } => write!(
                formatter,
                "casting job {} provider lacks maximum batch mass",
                job.value()
            ),
            Self::BatchMassExceedsEquipmentCapacity {
                job,
                selected,
                maximum,
            } => write!(
                formatter,
                "casting job {} batch {} mg exceeds provider capacity {} mg",
                job.value(),
                selected.milligrams(),
                maximum.milligrams()
            ),
            Self::Batch { job, error } => write!(
                formatter,
                "casting job {} batch cannot be reproduced: {error}",
                job.value()
            ),
            Self::InputTemperatureExceedsEquipmentMaximum {
                job,
                input,
                maximum,
            } => write!(
                formatter,
                "casting job {} input {} mK exceeds provider maximum {} mK",
                job.value(),
                input.millikelvin(),
                maximum.millikelvin()
            ),
            Self::WrongEnergyCarrier {
                job,
                required,
                provided,
            } => write!(
                formatter,
                "casting job {} releases {required:?} energy but traces {provided:?}",
                job.value()
            ),
            Self::ReleasedEnergyMismatch {
                job,
                traced,
                required,
            } => write!(
                formatter,
                "casting job {} traces {} nJ released but physics requires {} nJ",
                job.value(),
                traced.nanojoules(),
                required.nanojoules()
            ),
            Self::Duration { job, error } => write!(
                formatter,
                "casting job {} duration cannot be recomputed: {error}",
                job.value()
            ),
            Self::ConditionDuration { job, error } => write!(
                formatter,
                "casting job {} exceeds equipment condition lifetime: {error}",
                job.value()
            ),
            Self::DurationMismatch {
                job,
                stored,
                required,
            } => write!(
                formatter,
                "casting job {} stores {} ticks but physics requires {} ticks",
                job.value(),
                stored.value(),
                required.value()
            ),
            Self::MissingEquipmentConditionOutcome { job } => write!(
                formatter,
                "casting job {} has no post-operation equipment condition",
                job.value()
            ),
            Self::EquipmentConditionOutcomeMismatch {
                job,
                stored,
                required,
            } => write!(
                formatter,
                "casting job {} stores condition {} ppm but active-time wear requires {} ppm",
                job.value(),
                stored.parts_per_million(),
                required.parts_per_million()
            ),
            Self::OutputMismatch { job } => write!(
                formatter,
                "casting job {} solid output does not match its consumed liquid material",
                job.value()
            ),
        }
    }
}

impl Error for CastingJobValidationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Capability { error, .. } => Some(error),
            Self::Batch { error, .. } => Some(error),
            Self::Duration { error, .. } => Some(error),
            Self::ConditionDuration { error, .. } => Some(error),
            Self::UnexpectedConsumedEnergy { .. }
            | Self::MissingReleasedEnergy { .. }
            | Self::MissingEquipmentProvider { .. }
            | Self::UnknownEquipmentDefinition { .. }
            | Self::UnknownEnergyDefinition { .. }
            | Self::MissingCoolingPowerCapability { .. }
            | Self::MissingMaximumTemperatureCapability { .. }
            | Self::MissingMaximumBatchMassCapability { .. }
            | Self::BatchMassExceedsEquipmentCapacity { .. }
            | Self::InputTemperatureExceedsEquipmentMaximum { .. }
            | Self::WrongEnergyCarrier { .. }
            | Self::ReleasedEnergyMismatch { .. }
            | Self::DurationMismatch { .. }
            | Self::MissingEquipmentConditionOutcome { .. }
            | Self::EquipmentConditionOutcomeMismatch { .. }
            | Self::OutputMismatch { .. } => None,
        }
    }
}
