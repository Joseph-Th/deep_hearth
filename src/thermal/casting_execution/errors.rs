//! Public resolution and trusted-load validation errors for casting.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::capability::{CapabilityEvaluationError, CapabilityId};
use crate::core::quantity::{Energy, Mass, Temperature};
use crate::core::time::TickSpan;
use crate::energy::{EnergyCarrier, EnergySinkError, PowerDurationError};
use crate::equipment::EquipmentProviderError;
use crate::maintenance::{ActiveConditionDurationError, Condition};
use crate::production::{ProcessId, ProcessInputError, ProcessResolutionError, ProductionJobId};

use super::CastingBatchError;

/// Failure while resolving selected liquid matter into a conserved solid casting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CastingResolutionError {
    UnknownThermalProcess {
        process: ProcessId,
    },
    Input(ProcessInputError),
    Equipment(EquipmentProviderError),
    Capability(CapabilityEvaluationError),
    MissingCoolingPower {
        capability: CapabilityId,
    },
    MissingMaximumTemperature {
        capability: CapabilityId,
    },
    MissingMaximumBatchMass {
        capability: CapabilityId,
    },
    BatchMassExceedsEquipmentCapacity {
        selected: Mass,
        maximum: Mass,
    },
    Batch(CastingBatchError),
    InputTemperatureExceedsEquipmentMaximum {
        input: Temperature,
        maximum: Temperature,
    },
    EnergySink(EnergySinkError),
    WrongEnergyCarrier {
        required: EnergyCarrier,
        provided: EnergyCarrier,
    },
    Duration(PowerDurationError),
    ConditionDuration(ActiveConditionDurationError),
    Resolution(ProcessResolutionError),
}

impl Display for CastingResolutionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownThermalProcess { process } => write!(
                formatter,
                "process {} has no casting resolver definition",
                process.value()
            ),
            Self::Input(error) => write!(formatter, "process input binding failed: {error}"),
            Self::Equipment(error) => write!(formatter, "equipment resolution failed: {error}"),
            Self::Capability(error) => {
                write!(formatter, "equipment capability check failed: {error}")
            }
            Self::MissingCoolingPower { capability } => write!(
                formatter,
                "equipment does not expose configured cooling-power capability {}",
                capability.value()
            ),
            Self::MissingMaximumTemperature { capability } => write!(
                formatter,
                "equipment does not expose configured maximum-temperature capability {}",
                capability.value()
            ),
            Self::MissingMaximumBatchMass { capability } => write!(
                formatter,
                "equipment does not expose configured maximum-batch-mass capability {}",
                capability.value()
            ),
            Self::BatchMassExceedsEquipmentCapacity { selected, maximum } => write!(
                formatter,
                "selected batch {} mg exceeds equipment capacity {} mg",
                selected.milligrams(),
                maximum.milligrams()
            ),
            Self::Batch(error) => write!(formatter, "casting batch resolution failed: {error}"),
            Self::InputTemperatureExceedsEquipmentMaximum { input, maximum } => write!(
                formatter,
                "casting input temperature {} mK exceeds equipment maximum {} mK",
                input.millikelvin(),
                maximum.millikelvin()
            ),
            Self::EnergySink(error) => write!(formatter, "finite thermal sink failed: {error}"),
            Self::WrongEnergyCarrier { required, provided } => write!(
                formatter,
                "casting process releases {required:?} energy but sink stores {provided:?}"
            ),
            Self::Duration(error) => {
                write!(formatter, "casting duration calculation failed: {error}")
            }
            Self::ConditionDuration(error) => write!(
                formatter,
                "casting exceeds equipment condition lifetime: {error}"
            ),
            Self::Resolution(error) => write!(formatter, "process resolution failed: {error}"),
        }
    }
}

impl Error for CastingResolutionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Input(error) => Some(error),
            Self::Equipment(error) => Some(error),
            Self::Capability(error) => Some(error),
            Self::Batch(error) => Some(error),
            Self::EnergySink(error) => Some(error),
            Self::Duration(error) => Some(error),
            Self::ConditionDuration(error) => Some(error),
            Self::Resolution(error) => Some(error),
            Self::UnknownThermalProcess { .. }
            | Self::MissingCoolingPower { .. }
            | Self::MissingMaximumTemperature { .. }
            | Self::MissingMaximumBatchMass { .. }
            | Self::BatchMassExceedsEquipmentCapacity { .. }
            | Self::InputTemperatureExceedsEquipmentMaximum { .. }
            | Self::WrongEnergyCarrier { .. } => None,
        }
    }
}

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
