//! Public validation and commit errors for mining start admission.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::capability::{CapabilityId, CapabilityValueKind};
use crate::core::quantity::{Mass, Pressure};
use crate::equipment::{EquipmentId, EquipmentProviderError};
use crate::inventory::{StockpileId, StockpileStorageError, StockpileStructuralLoadError};
use crate::labor::{PlayerWorkCommitError, PlayerWorkStartError};
use crate::maintenance::ActiveConditionDurationError;
use crate::material::MaterialLotSpecError;
use crate::ore_processing::MassFlowDurationError;
use crate::production::{ProductionJobId, ProductionOccupancyRelease};

use super::super::physics::MiningPhysicsError;
use super::super::{MiningJobId, MiningMethodId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MiningStartError {
    UnknownMethod {
        method: MiningMethodId,
    },
    TargetNoLongerResolved,
    ZeroMass,
    InsufficientTargetMass {
        requested: Mass,
    },
    Equipment(EquipmentProviderError),
    EquipmentMounted {
        equipment: EquipmentId,
    },
    EquipmentBusyProduction {
        equipment: EquipmentId,
        job: ProductionJobId,
        release: ProductionOccupancyRelease,
    },
    EquipmentBusyMining {
        equipment: EquipmentId,
        job: MiningJobId,
    },
    MissingCapability {
        capability: CapabilityId,
    },
    CapabilityKindMismatch {
        capability: CapabilityId,
        expected: CapabilityValueKind,
        found: CapabilityValueKind,
    },
    BatchTooLarge {
        maximum: Mass,
        requested: Mass,
    },
    TargetTooHard {
        maximum: Pressure,
    },
    ZeroThroughput,
    Duration(MassFlowDurationError),
    ConditionDuration(ActiveConditionDurationError),
    CompletionTickOverflow,
    InvalidOutput(MaterialLotSpecError),
    UnknownDestination {
        stockpile: StockpileId,
    },
    DestinationBusyStorageDismantling {
        stockpile: StockpileId,
    },
    DestinationStorage(StockpileStorageError),
    DestinationMassOverflow {
        stockpile: StockpileId,
    },
    DestinationCapacityExceeded {
        stockpile: StockpileId,
        capacity: Mass,
        committed: Mass,
        requested: Mass,
    },
    InventoryRevisionExhausted,
    DestinationSupport(StockpileStructuralLoadError),
    MiningIdExhausted,
    MiningRevisionExhausted,
    Work(PlayerWorkStartError),
}

impl Display for MiningStartError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownMethod { method } => {
                write!(formatter, "unknown mining method {}", method.value())
            }
            Self::TargetNoLongerResolved => formatter.write_str(
                "resolved mining target is no longer uniquely supported by current local evidence and geology",
            ),
            Self::ZeroMass => formatter.write_str("mining request mass must be nonzero"),
            Self::InsufficientTargetMass { requested } => write!(
                formatter,
                "resolved mining target cannot supply the requested {} mg",
                requested.milligrams()
            ),
            Self::Equipment(error) => write!(formatter, "mining equipment failed: {error}"),
            Self::EquipmentMounted { equipment } => write!(
                formatter,
                "mining equipment {} is mounted and cannot be used for extraction",
                equipment.value()
            ),
            Self::EquipmentBusyProduction {
                equipment,
                job,
                release,
            } => write!(
                formatter,
                "mining equipment {} is occupied by production job {} {release}",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyMining { equipment, job } => write!(
                formatter,
                "mining equipment {} is occupied by mining job {}",
                equipment.value(),
                job.value()
            ),
            Self::MissingCapability { capability } => write!(
                formatter,
                "mining equipment lacks required capability {}",
                capability.value()
            ),
            Self::CapabilityKindMismatch {
                capability,
                expected,
                found,
            } => write!(
                formatter,
                "mining capability {} has {found:?} value kind instead of {expected:?}",
                capability.value()
            ),
            Self::BatchTooLarge { maximum, requested } => write!(
                formatter,
                "mining batch {} mg exceeds equipment maximum {} mg",
                requested.milligrams(),
                maximum.milligrams()
            ),
            Self::TargetTooHard { maximum } => write!(
                formatter,
                "resolved mining target exceeds equipment maximum excavation hardness {} Pa",
                maximum.pascals()
            ),
            Self::ZeroThroughput => formatter.write_str("resolved mining throughput is zero"),
            Self::Duration(error) => write!(formatter, "mining duration resolution failed: {error}"),
            Self::ConditionDuration(error) => write!(
                formatter,
                "mining exceeds equipment condition lifetime: {error}"
            ),
            Self::CompletionTickOverflow => {
                formatter.write_str("mining completion exceeds the world clock range")
            }
            Self::InvalidOutput(error) => write!(formatter, "mining output is invalid: {error}"),
            Self::UnknownDestination { stockpile } => write!(
                formatter,
                "unknown mining destination stockpile {}",
                stockpile.value()
            ),
            Self::DestinationBusyStorageDismantling { stockpile } => write!(
                formatter,
                "stockpile {} is being dismantled and cannot reserve mining output",
                stockpile.value()
            ),
            Self::DestinationStorage(error) => {
                write!(formatter, "mining destination rejects output: {error}")
            }
            Self::DestinationMassOverflow { stockpile } => write!(
                formatter,
                "mining output mass overflows destination stockpile {}",
                stockpile.value()
            ),
            Self::DestinationCapacityExceeded {
                stockpile,
                capacity,
                committed,
                requested,
            } => write!(
                formatter,
                "stockpile {} capacity {} mg cannot reserve {} mg with {} mg already committed",
                stockpile.value(),
                capacity.milligrams(),
                requested.milligrams(),
                committed.milligrams()
            ),
            Self::InventoryRevisionExhausted => {
                formatter.write_str("inventory revision space is exhausted")
            }
            Self::DestinationSupport(error) => {
                write!(formatter, "mining destination support failed: {error}")
            }
            Self::MiningIdExhausted => {
                formatter.write_str("mining job identifier space is exhausted")
            }
            Self::MiningRevisionExhausted => {
                formatter.write_str("mining revision space is exhausted")
            }
            Self::Work(error) => write!(formatter, "mining player-work admission failed: {error}"),
        }
    }
}

impl Error for MiningStartError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Equipment(error) => Some(error),
            Self::Duration(error) => Some(error),
            Self::ConditionDuration(error) => Some(error),
            Self::InvalidOutput(error) => Some(error),
            Self::DestinationStorage(error) => Some(error),
            Self::DestinationSupport(error) => Some(error),
            Self::Work(error) => Some(error),
            Self::UnknownMethod { .. }
            | Self::TargetNoLongerResolved
            | Self::ZeroMass
            | Self::InsufficientTargetMass { .. }
            | Self::EquipmentMounted { .. }
            | Self::EquipmentBusyProduction { .. }
            | Self::EquipmentBusyMining { .. }
            | Self::MissingCapability { .. }
            | Self::CapabilityKindMismatch { .. }
            | Self::BatchTooLarge { .. }
            | Self::TargetTooHard { .. }
            | Self::ZeroThroughput
            | Self::CompletionTickOverflow
            | Self::UnknownDestination { .. }
            | Self::DestinationBusyStorageDismantling { .. }
            | Self::DestinationMassOverflow { .. }
            | Self::DestinationCapacityExceeded { .. }
            | Self::InventoryRevisionExhausted
            | Self::MiningIdExhausted
            | Self::MiningRevisionExhausted => None,
        }
    }
}

impl From<MiningPhysicsError> for MiningStartError {
    fn from(error: MiningPhysicsError) -> Self {
        match error {
            MiningPhysicsError::MissingCapability { capability } => {
                Self::MissingCapability { capability }
            }
            MiningPhysicsError::CapabilityKindMismatch {
                capability,
                expected,
                found,
            } => Self::CapabilityKindMismatch {
                capability,
                expected,
                found,
            },
            MiningPhysicsError::BatchTooLarge { maximum, requested } => {
                Self::BatchTooLarge { maximum, requested }
            }
            MiningPhysicsError::DepositTooHard { maximum, .. } => Self::TargetTooHard { maximum },
            MiningPhysicsError::ZeroThroughput => Self::ZeroThroughput,
            MiningPhysicsError::Duration(error) => Self::Duration(error),
            MiningPhysicsError::ConditionDuration(error) => Self::ConditionDuration(error),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MiningStartCommitError {
    TargetNoLongerResolved,
    TargetMassChanged {
        expected: Mass,
        actual: Mass,
    },
    StaleInventory {
        expected: u64,
        actual: u64,
    },
    StaleEquipment {
        expected: u64,
        actual: u64,
    },
    StaleMining {
        expected: u64,
        actual: u64,
    },
    StaleStructure {
        expected: u64,
        actual: u64,
    },
    EquipmentBusyProduction {
        equipment: EquipmentId,
        job: ProductionJobId,
    },
    EquipmentBusyMining {
        equipment: EquipmentId,
        job: MiningJobId,
    },
    Work(PlayerWorkCommitError),
}

impl Display for MiningStartCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TargetNoLongerResolved => formatter.write_str(
                "validated mining target is no longer uniquely supported by current local evidence and geology",
            ),
            Self::TargetMassChanged { expected, actual } => write!(
                formatter,
                "validated mining target source mass changed from {} mg to {} mg before commit",
                expected.milligrams(),
                actual.milligrams()
            ),
            Self::StaleInventory { expected, actual } => write!(
                formatter,
                "validated mining start expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::StaleEquipment { expected, actual } => write!(
                formatter,
                "validated mining start expected equipment revision {expected} but current revision is {actual}"
            ),
            Self::StaleMining { expected, actual } => write!(
                formatter,
                "validated mining start expected mining revision {expected} but current revision is {actual}"
            ),
            Self::StaleStructure { expected, actual } => write!(
                formatter,
                "validated mining start expected structural revision {expected} but current revision is {actual}"
            ),
            Self::EquipmentBusyProduction { equipment, job } => write!(
                formatter,
                "validated mining start equipment {} became occupied by production job {}",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyMining { equipment, job } => write!(
                formatter,
                "validated mining start equipment {} became occupied by mining job {}",
                equipment.value(),
                job.value()
            ),
            Self::Work(error) => write!(
                formatter,
                "validated mining start player-work state changed: {error}"
            ),
        }
    }
}

impl Error for MiningStartCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Work(error) => Some(error),
            Self::TargetNoLongerResolved
            | Self::TargetMassChanged { .. }
            | Self::StaleInventory { .. }
            | Self::StaleEquipment { .. }
            | Self::StaleMining { .. }
            | Self::StaleStructure { .. }
            | Self::EquipmentBusyProduction { .. }
            | Self::EquipmentBusyMining { .. } => None,
        }
    }
}
