//! Equipment-disassembly validation and commit failures.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::inventory::{StockpileId, StockpileStorageError, StockpileStructuralLoadError};
use crate::maintenance::Condition;
use crate::mining::MiningJobId;
use crate::production::{ProductionJobId, ProductionOccupancyRelease};
use crate::structural::{StructuralCommitError, StructuralElementId};

use super::super::EquipmentId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EquipmentDisassemblyError {
    UnknownEquipment {
        equipment: EquipmentId,
    },
    NoEmbodiedMatter {
        equipment: EquipmentId,
    },
    WornRecoveryUnavailable {
        equipment: EquipmentId,
        condition: Condition,
    },
    EquipmentMounted {
        equipment: EquipmentId,
        element: StructuralElementId,
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
    UnknownDestination {
        stockpile: StockpileId,
    },
    InvalidEmbodiedMatter {
        equipment: EquipmentId,
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
    LotIdExhausted,
    InventoryRevisionExhausted,
    EquipmentRevisionExhausted,
    StoredMatterLoad(StockpileStructuralLoadError),
}

impl Display for EquipmentDisassemblyError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownEquipment { equipment } => {
                write!(formatter, "unknown equipment id {}", equipment.value())
            }
            Self::NoEmbodiedMatter { equipment } => write!(
                formatter,
                "equipment {} has no embodied matter to disassemble",
                equipment.value()
            ),
            Self::WornRecoveryUnavailable {
                equipment,
                condition,
            } => write!(
                formatter,
                "equipment {} is at {} ppm condition and its definition has no destructive worn-recovery form",
                equipment.value(),
                condition.parts_per_million()
            ),
            Self::EquipmentMounted { equipment, element } => write!(
                formatter,
                "equipment {} is mounted on structural element {} and must be unmounted before disassembly",
                equipment.value(),
                element.value()
            ),
            Self::EquipmentBusyProduction {
                equipment,
                job,
                release,
            } => write!(
                formatter,
                "equipment {} is occupied by production job {} {release} and cannot be disassembled",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyMining { equipment, job } => write!(
                formatter,
                "equipment {} is occupied by mining job {} and cannot be disassembled",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyManualPower { equipment } => write!(
                formatter,
                "equipment {} is occupied by direct player-powered generation and cannot be disassembled",
                equipment.value()
            ),
            Self::UnknownDestination { stockpile } => write!(
                formatter,
                "equipment disassembly destination stockpile {} does not exist",
                stockpile.value()
            ),
            Self::InvalidEmbodiedMatter { equipment } => write!(
                formatter,
                "equipment {} contains embodied matter that cannot re-enter inventory",
                equipment.value()
            ),
            Self::DestinationStorage(error) => write!(
                formatter,
                "equipment disassembly destination rejects recovered material: {error}"
            ),
            Self::DestinationMassOverflow { stockpile } => write!(
                formatter,
                "equipment disassembly overflows stockpile {} mass accounting",
                stockpile.value()
            ),
            Self::DestinationCapacityExceeded {
                stockpile,
                capacity,
                committed,
                requested,
            } => write!(
                formatter,
                "equipment disassembly exceeds stockpile {} capacity {} mg: {} mg committed, {} mg requested",
                stockpile.value(),
                capacity.milligrams(),
                committed.milligrams(),
                requested.milligrams()
            ),
            Self::LotIdExhausted => {
                formatter.write_str("material lot identifier space is exhausted during disassembly")
            }
            Self::InventoryRevisionExhausted => {
                formatter.write_str("inventory revision space is exhausted during disassembly")
            }
            Self::EquipmentRevisionExhausted => {
                formatter.write_str("equipment revision space is exhausted during disassembly")
            }
            Self::StoredMatterLoad(error) => write!(
                formatter,
                "equipment disassembly cannot update destination stored-matter load: {error}"
            ),
        }
    }
}

impl Error for EquipmentDisassemblyError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::DestinationStorage(error) => Some(error),
            Self::StoredMatterLoad(error) => Some(error),
            Self::UnknownEquipment { .. }
            | Self::NoEmbodiedMatter { .. }
            | Self::WornRecoveryUnavailable { .. }
            | Self::EquipmentMounted { .. }
            | Self::EquipmentBusyProduction { .. }
            | Self::EquipmentBusyMining { .. }
            | Self::EquipmentBusyManualPower { .. }
            | Self::UnknownDestination { .. }
            | Self::InvalidEmbodiedMatter { .. }
            | Self::DestinationMassOverflow { .. }
            | Self::DestinationCapacityExceeded { .. }
            | Self::LotIdExhausted
            | Self::InventoryRevisionExhausted
            | Self::EquipmentRevisionExhausted => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EquipmentDisassemblyCommitError {
    StaleInventory {
        expected: u64,
        actual: u64,
    },
    StaleEquipment {
        expected: u64,
        actual: u64,
    },
    UnknownEquipment {
        equipment: EquipmentId,
    },
    EquipmentChanged {
        equipment: EquipmentId,
    },
    EquipmentMounted {
        equipment: EquipmentId,
        element: StructuralElementId,
    },
    EquipmentBusyProduction {
        equipment: EquipmentId,
        job: ProductionJobId,
    },
    EquipmentBusyMining {
        equipment: EquipmentId,
        job: MiningJobId,
    },
    EquipmentBusyManualPower {
        equipment: EquipmentId,
    },
    Structure(StructuralCommitError),
}

impl Display for EquipmentDisassemblyCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleInventory { expected, actual } => write!(
                formatter,
                "equipment disassembly expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::StaleEquipment { expected, actual } => write!(
                formatter,
                "equipment disassembly expected equipment revision {expected} but current revision is {actual}"
            ),
            Self::UnknownEquipment { equipment } => write!(
                formatter,
                "equipment {} disappeared before disassembly commit",
                equipment.value()
            ),
            Self::EquipmentChanged { equipment } => write!(
                formatter,
                "equipment {} changed after disassembly validation",
                equipment.value()
            ),
            Self::EquipmentMounted { equipment, element } => write!(
                formatter,
                "equipment {} became mounted on structural element {} before disassembly commit",
                equipment.value(),
                element.value()
            ),
            Self::EquipmentBusyProduction { equipment, job } => write!(
                formatter,
                "equipment {} became occupied by production job {} before disassembly commit",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyMining { equipment, job } => write!(
                formatter,
                "equipment {} became occupied by mining job {} before disassembly commit",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyManualPower { equipment } => write!(
                formatter,
                "equipment {} became occupied by direct player-powered generation before disassembly commit",
                equipment.value()
            ),
            Self::Structure(error) => {
                write!(formatter, "equipment disassembly structure failed: {error}")
            }
        }
    }
}

impl Error for EquipmentDisassemblyCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Structure(error) => Some(error),
            Self::StaleInventory { .. }
            | Self::StaleEquipment { .. }
            | Self::UnknownEquipment { .. }
            | Self::EquipmentChanged { .. }
            | Self::EquipmentMounted { .. }
            | Self::EquipmentBusyProduction { .. }
            | Self::EquipmentBusyMining { .. }
            | Self::EquipmentBusyManualPower { .. } => None,
        }
    }
}
