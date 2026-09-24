//! Error vocabulary for bounded powered-ore order projection.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::capability::CapabilityEvaluationError;
use crate::core::throughput::MassFlowDurationError;
use crate::energy::{EnergyCarrier, EnergyStoreDefinitionId, PowerDurationError};
use crate::equipment::EquipmentDefinitionId;
use crate::maintenance::{ActiveConditionDurationError, Condition};
use crate::production::ProcessId;

/// Failure to project a bounded powered-ore order from immutable authored physics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PoweredOreOrderError {
    ZeroRequestedMass,
    BatchLimitExceeded {
        maximum: u64,
    },
    UnknownPoweredProcess {
        process: ProcessId,
    },
    UnknownEquipment {
        equipment: EquipmentDefinitionId,
    },
    UnknownEnergyStore {
        store: EnergyStoreDefinitionId,
    },
    WrongEnergyCarrier {
        required: EnergyCarrier,
        provided: EnergyCarrier,
    },
    Capability {
        batch: u64,
        error: CapabilityEvaluationError,
    },
    MissingMassFlowCapability {
        batch: u64,
    },
    MissingMaximumBatchMassCapability {
        batch: u64,
    },
    NoBatchCapacity {
        batch: u64,
        condition: Condition,
    },
    MaintenanceUnavailable {
        equipment: EquipmentDefinitionId,
    },
    ThroughputDuration {
        batch: u64,
        error: MassFlowDurationError,
    },
    EnergyDuration {
        batch: u64,
        error: PowerDurationError,
    },
    ConditionDuration {
        batch: u64,
        error: ActiveConditionDurationError,
    },
    DurationOverflow,
}

impl Display for PoweredOreOrderError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroRequestedMass => {
                formatter.write_str("powered ore order mass must be nonzero")
            }
            Self::BatchLimitExceeded { maximum } => write!(
                formatter,
                "powered ore order exceeds the caller's {maximum}-batch projection bound"
            ),
            Self::UnknownPoweredProcess { process } => write!(
                formatter,
                "process {} has no authored powered ore-processing profile",
                process.value()
            ),
            Self::UnknownEquipment { equipment } => write!(
                formatter,
                "unknown equipment definition {}",
                equipment.value()
            ),
            Self::UnknownEnergyStore { store } => write!(
                formatter,
                "unknown energy-store definition {}",
                store.value()
            ),
            Self::WrongEnergyCarrier { required, provided } => write!(
                formatter,
                "powered ore process requires {required:?} energy but replenished store provides {provided:?}"
            ),
            Self::Capability { batch, error } => write!(
                formatter,
                "powered ore order batch {batch} capability failed: {error}"
            ),
            Self::MissingMassFlowCapability { batch } => write!(
                formatter,
                "powered ore order batch {batch} lacks its authored mass-flow capability"
            ),
            Self::MissingMaximumBatchMassCapability { batch } => write!(
                formatter,
                "powered ore order batch {batch} lacks its authored maximum-batch capability"
            ),
            Self::NoBatchCapacity { batch, condition } => write!(
                formatter,
                "powered ore order batch {batch} has no positive capacity at {} ppm condition",
                condition.parts_per_million()
            ),
            Self::MaintenanceUnavailable { equipment } => write!(
                formatter,
                "equipment definition {} entered its critical band without an authored maintenance profile",
                equipment.value()
            ),
            Self::ThroughputDuration { batch, error } => write!(
                formatter,
                "powered ore order batch {batch} throughput duration failed: {error}"
            ),
            Self::EnergyDuration { batch, error } => write!(
                formatter,
                "powered ore order batch {batch} energy duration failed: {error}"
            ),
            Self::ConditionDuration { batch, error } => write!(
                formatter,
                "powered ore order batch {batch} condition failed: {error}"
            ),
            Self::DurationOverflow => {
                formatter.write_str("powered ore order active duration exceeds tick range")
            }
        }
    }
}

impl Error for PoweredOreOrderError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Capability { error, .. } => Some(error),
            Self::ThroughputDuration { error, .. } => Some(error),
            Self::EnergyDuration { error, .. } => Some(error),
            Self::ConditionDuration { error, .. } => Some(error),
            Self::ZeroRequestedMass
            | Self::BatchLimitExceeded { .. }
            | Self::UnknownPoweredProcess { .. }
            | Self::UnknownEquipment { .. }
            | Self::UnknownEnergyStore { .. }
            | Self::WrongEnergyCarrier { .. }
            | Self::MissingMassFlowCapability { .. }
            | Self::MissingMaximumBatchMassCapability { .. }
            | Self::NoBatchCapacity { .. }
            | Self::MaintenanceUnavailable { .. }
            | Self::DurationOverflow => None,
        }
    }
}
