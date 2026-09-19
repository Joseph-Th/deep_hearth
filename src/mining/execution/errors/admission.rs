//! Admission failures for resolving and validating one mining start.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::capability::{CapabilityId, CapabilityValueKind};
use crate::core::quantity::{Mass, Pressure};
use crate::core::throughput::MassFlowDurationError;
use crate::equipment::{EquipmentId, EquipmentProviderError};
use crate::inventory::{StockpileId, StockpileStorageError, StockpileStructuralLoadError};
use crate::labor::PlayerWorkStartError;
use crate::maintenance::ActiveConditionDurationError;
use crate::material::{MaterialId, MaterialLotSpecError};
use crate::production::{ProductionJobId, ProductionOccupancyRelease};
use crate::spatial::VoxelBounds;

use super::super::super::physics::MiningPhysicsError;
use super::super::super::{MiningJobId, MiningMethodId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MiningStartError {
    UnknownMethod {
        method: MiningMethodId,
    },
    TargetNoLongerResolved,
    MissingExcavationHardnessEvidence {
        material: MaterialId,
        region: VoxelBounds,
    },
    ZeroMass,
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
    EquipmentBusyManualPower {
        equipment: EquipmentId,
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
    ExcavationHardnessEvidenceExceedsCapability {
        observed_upper: Pressure,
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
    GeologyRevisionExhausted,
    EquipmentRevisionExhausted,
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
            Self::MissingExcavationHardnessEvidence { material, .. } => write!(
                formatter,
                "mining target for material {} lacks acquired excavation-hardness evidence; perform physical sampling before extraction",
                material.value()
            ),
            Self::ZeroMass => formatter.write_str("mining request mass must be nonzero"),
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
            Self::EquipmentBusyManualPower { equipment } => write!(
                formatter,
                "mining equipment {} is occupied by manual power generation",
                equipment.value()
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
            Self::ExcavationHardnessEvidenceExceedsCapability {
                observed_upper,
                maximum,
            } => write!(
                formatter,
                "acquired excavation-hardness upper bound {} Pa exceeds equipment maximum {} Pa",
                observed_upper.pascals(),
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
            Self::GeologyRevisionExhausted => {
                formatter.write_str("geology revision space is exhausted")
            }
            Self::EquipmentRevisionExhausted => {
                formatter.write_str("equipment revision space is exhausted")
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
            | Self::MissingExcavationHardnessEvidence { .. }
            | Self::ZeroMass
            | Self::EquipmentMounted { .. }
            | Self::EquipmentBusyProduction { .. }
            | Self::EquipmentBusyMining { .. }
            | Self::EquipmentBusyManualPower { .. }
            | Self::MissingCapability { .. }
            | Self::CapabilityKindMismatch { .. }
            | Self::BatchTooLarge { .. }
            | Self::ExcavationHardnessEvidenceExceedsCapability { .. }
            | Self::ZeroThroughput
            | Self::CompletionTickOverflow
            | Self::UnknownDestination { .. }
            | Self::DestinationBusyStorageDismantling { .. }
            | Self::DestinationMassOverflow { .. }
            | Self::DestinationCapacityExceeded { .. }
            | Self::InventoryRevisionExhausted
            | Self::MiningIdExhausted
            | Self::MiningRevisionExhausted
            | Self::GeologyRevisionExhausted
            | Self::EquipmentRevisionExhausted => None,
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
            MiningPhysicsError::DepositTooHard { hardness, maximum } => {
                Self::ExcavationHardnessEvidenceExceedsCapability {
                    observed_upper: hardness,
                    maximum,
                }
            }
            MiningPhysicsError::ZeroThroughput => Self::ZeroThroughput,
            MiningPhysicsError::Duration(error) => Self::Duration(error),
            MiningPhysicsError::ConditionDuration(error) => Self::ConditionDuration(error),
        }
    }
}
