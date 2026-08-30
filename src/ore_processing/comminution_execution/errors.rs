//! Resolution and trusted-load failures for comminution operations.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::capability::CapabilityEvaluationError;
use crate::core::quantity::Mass;
use crate::core::time::TickSpan;
use crate::energy::{EnergyCarrier, EnergySupplyError, PowerDurationError};
use crate::equipment::EquipmentProviderError;
use crate::maintenance::ActiveConditionDurationError;
use crate::production::{ProcessId, ProcessInputError, ProcessResolutionError, ProductionJobId};

use crate::ore_processing::MassFlowDurationError;
use crate::ore_processing::powered_physics::PoweredOreJobValidationError;

use super::outputs::ComminutionBatchError;

/// Failure while resolving one exact comminution operation before any authoritative mutation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComminutionResolutionError {
    UnknownComminutionProcess {
        process: ProcessId,
    },
    Input(ProcessInputError),
    Equipment(EquipmentProviderError),
    Capability(CapabilityEvaluationError),
    MissingMassFlowCapability,
    MissingMaximumBatchMassCapability,
    BatchMassExceeded {
        selected: Mass,
        maximum: Mass,
    },
    Batch(ComminutionBatchError),
    Energy(EnergySupplyError),
    WrongEnergyCarrier {
        required: EnergyCarrier,
        provided: EnergyCarrier,
    },
    ThroughputDuration(MassFlowDurationError),
    EnergyDuration(PowerDurationError),
    ConditionDuration(ActiveConditionDurationError),
    Resolution(ProcessResolutionError),
}

impl Display for ComminutionResolutionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownComminutionProcess { process } => write!(
                formatter,
                "process {} has no authored comminution semantics",
                process.value()
            ),
            Self::Input(error) => write!(formatter, "comminution input selection failed: {error}"),
            Self::Equipment(error) => write!(formatter, "comminution equipment failed: {error}"),
            Self::Capability(error) => write!(
                formatter,
                "comminution capability requirement failed: {error}"
            ),
            Self::MissingMassFlowCapability => {
                formatter.write_str("comminution equipment has no usable mass-flow capability")
            }
            Self::MissingMaximumBatchMassCapability => formatter
                .write_str("comminution equipment has no usable maximum-batch-mass capability"),
            Self::BatchMassExceeded { selected, maximum } => write!(
                formatter,
                "selected comminution batch {} mg exceeds equipment maximum {} mg",
                selected.milligrams(),
                maximum.milligrams()
            ),
            Self::Batch(error) => write!(formatter, "comminution batch resolution failed: {error}"),
            Self::Energy(error) => write!(formatter, "comminution energy supply failed: {error}"),
            Self::WrongEnergyCarrier { required, provided } => write!(
                formatter,
                "comminution requires {required:?} energy but selected source provides {provided:?}"
            ),
            Self::ThroughputDuration(error) => {
                write!(formatter, "comminution throughput duration failed: {error}")
            }
            Self::EnergyDuration(error) => write!(
                formatter,
                "comminution energy delivery duration failed: {error}"
            ),
            Self::ConditionDuration(error) => write!(
                formatter,
                "comminution exceeds equipment condition lifetime: {error}"
            ),
            Self::Resolution(error) => {
                write!(formatter, "comminution process resolution failed: {error}")
            }
        }
    }
}

impl Error for ComminutionResolutionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Input(error) => Some(error),
            Self::Equipment(error) => Some(error),
            Self::Capability(error) => Some(error),
            Self::Batch(error) => Some(error),
            Self::Energy(error) => Some(error),
            Self::ThroughputDuration(error) => Some(error),
            Self::EnergyDuration(error) => Some(error),
            Self::ConditionDuration(error) => Some(error),
            Self::Resolution(error) => Some(error),
            Self::UnknownComminutionProcess { .. }
            | Self::MissingMassFlowCapability
            | Self::MissingMaximumBatchMassCapability
            | Self::BatchMassExceeded { .. }
            | Self::WrongEnergyCarrier { .. } => None,
        }
    }
}

/// Persistent-state failure found while recomputing an in-flight comminution job from its traces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComminutionJobValidationError {
    Powered {
        job: ProductionJobId,
        error: PoweredOreJobValidationError,
    },
    Batch {
        job: ProductionJobId,
        error: ComminutionBatchError,
    },
    ManualUnexpectedEnergy {
        job: ProductionJobId,
    },
    ManualUnexpectedEquipment {
        job: ProductionJobId,
    },
    ManualBatchMassExceeded {
        job: ProductionJobId,
        selected: Mass,
        maximum: Mass,
    },
    ManualDuration {
        job: ProductionJobId,
        error: MassFlowDurationError,
    },
    ManualDurationMismatch {
        job: ProductionJobId,
        stored: TickSpan,
        required: TickSpan,
    },
    OutputMismatch {
        job: ProductionJobId,
    },
}

impl Display for ComminutionJobValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Powered { job, error } => write!(
                formatter,
                "comminution job {} powered-physics replay failed: {error}",
                job.value()
            ),
            Self::Batch { job, error } => write!(
                formatter,
                "comminution job {} has invalid batch physics: {error}",
                job.value()
            ),
            Self::ManualUnexpectedEnergy { job } => write!(
                formatter,
                "manual comminution job {} carries unauthored energy",
                job.value()
            ),
            Self::ManualUnexpectedEquipment { job } => write!(
                formatter,
                "manual comminution job {} carries unauthored equipment",
                job.value()
            ),
            Self::ManualBatchMassExceeded {
                job,
                selected,
                maximum,
            } => write!(
                formatter,
                "manual comminution job {} contains {} mg beyond its {} mg hand-breaking limit",
                job.value(),
                selected.milligrams(),
                maximum.milligrams()
            ),
            Self::ManualDuration { job, error } => write!(
                formatter,
                "manual comminution job {} duration replay failed: {error}",
                job.value()
            ),
            Self::ManualDurationMismatch {
                job,
                stored,
                required,
            } => write!(
                formatter,
                "manual comminution job {} stores {} active ticks but requires {}",
                job.value(),
                stored.value(),
                required.value()
            ),
            Self::OutputMismatch { job } => write!(
                formatter,
                "comminution job {} output snapshot no longer matches its consumed material traces",
                job.value()
            ),
        }
    }
}

impl Error for ComminutionJobValidationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Powered { error, .. } => Some(error),
            Self::Batch { error, .. } => Some(error),
            Self::ManualDuration { error, .. } => Some(error),
            Self::ManualUnexpectedEnergy { .. }
            | Self::ManualUnexpectedEquipment { .. }
            | Self::ManualBatchMassExceeded { .. }
            | Self::ManualDurationMismatch { .. }
            | Self::OutputMismatch { .. } => None,
        }
    }
}
