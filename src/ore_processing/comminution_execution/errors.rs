//! Runtime resolution failures for powered comminution operations.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::capability::CapabilityEvaluationError;
use crate::core::quantity::Mass;
use crate::core::throughput::MassFlowDurationError;
use crate::energy::{EnergyCarrier, EnergySupplyError, PowerDurationError};
use crate::equipment::EquipmentProviderError;
use crate::maintenance::ActiveConditionDurationError;
use crate::production::{ProcessId, ProcessInputError, ProcessResolutionError};

use crate::ore_processing::powered_physics::{
    PoweredOreEquipmentError, PoweredOreProviderError, PoweredOreSupplyError, PoweredOreTimingError,
};

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

impl From<PoweredOreProviderError> for ComminutionResolutionError {
    fn from(error: PoweredOreProviderError) -> Self {
        match error {
            PoweredOreProviderError::UnknownProcess { process } => {
                Self::UnknownComminutionProcess { process }
            }
            PoweredOreProviderError::Provider(error) => Self::Equipment(error),
            PoweredOreProviderError::Capability(error) => Self::Capability(error),
            PoweredOreProviderError::Equipment(error) => match error {
                PoweredOreEquipmentError::MissingMassFlowCapability => {
                    Self::MissingMassFlowCapability
                }
                PoweredOreEquipmentError::MissingMaximumBatchMassCapability => {
                    Self::MissingMaximumBatchMassCapability
                }
                PoweredOreEquipmentError::BatchMassExceeded { selected, maximum } => {
                    Self::BatchMassExceeded { selected, maximum }
                }
            },
        }
    }
}

impl From<PoweredOreSupplyError> for ComminutionResolutionError {
    fn from(error: PoweredOreSupplyError) -> Self {
        match error {
            PoweredOreSupplyError::Supply(error) => Self::Energy(error),
            PoweredOreSupplyError::WrongEnergyCarrier { required, provided } => {
                Self::WrongEnergyCarrier { required, provided }
            }
            PoweredOreSupplyError::Timing(error) => match error {
                PoweredOreTimingError::Throughput(error) => Self::ThroughputDuration(error),
                PoweredOreTimingError::Energy(error) => Self::EnergyDuration(error),
                PoweredOreTimingError::Condition(error) => Self::ConditionDuration(error),
            },
        }
    }
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
