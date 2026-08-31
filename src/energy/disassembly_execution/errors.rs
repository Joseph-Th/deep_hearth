//! Diagnostics for energy-store disassembly validation and commit conflicts.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Energy, Mass};
use crate::inventory::{StockpileId, StockpileStorageError, StockpileStructuralLoadError};
use crate::production::{ProductionJobId, ProductionOccupancyRelease};
use crate::structural::StructuralCommitError;

use super::super::EnergyStoreId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EnergyStoreDisassemblyError {
    UnknownStore {
        store: EnergyStoreId,
    },
    NoEmbodiedMatter {
        store: EnergyStoreId,
    },
    StoreNotEmpty {
        store: EnergyStoreId,
        stored: Energy,
    },
    StoreBusyProduction {
        store: EnergyStoreId,
        job: ProductionJobId,
        release: ProductionOccupancyRelease,
    },
    StoreBusyManualPower {
        store: EnergyStoreId,
    },
    UnknownDestination {
        stockpile: StockpileId,
    },
    InvalidEmbodiedMatter {
        store: EnergyStoreId,
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
    EnergyRevisionExhausted,
    StoredMatterLoad(StockpileStructuralLoadError),
}

impl Display for EnergyStoreDisassemblyError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownStore { store } => {
                write!(formatter, "unknown energy store {}", store.value())
            }
            Self::NoEmbodiedMatter { store } => write!(
                formatter,
                "energy store {} has no embodied matter to disassemble",
                store.value()
            ),
            Self::StoreNotEmpty { store, stored } => write!(
                formatter,
                "energy store {} still owns {} nJ and cannot be disassembled",
                store.value(),
                stored.nanojoules()
            ),
            Self::StoreBusyProduction {
                store,
                job,
                release,
            } => write!(
                formatter,
                "energy store {} is occupied by production job {} {release} and cannot be disassembled",
                store.value(),
                job.value()
            ),
            Self::StoreBusyManualPower { store } => write!(
                formatter,
                "energy store {} is reserved by direct player-powered generation and cannot be disassembled",
                store.value()
            ),
            Self::UnknownDestination { stockpile } => write!(
                formatter,
                "energy-store disassembly destination stockpile {} does not exist",
                stockpile.value()
            ),
            Self::InvalidEmbodiedMatter { store } => write!(
                formatter,
                "energy store {} contains embodied matter that cannot re-enter inventory",
                store.value()
            ),
            Self::DestinationStorage(error) => write!(
                formatter,
                "energy-store disassembly destination rejects recovered material: {error}"
            ),
            Self::DestinationMassOverflow { stockpile } => write!(
                formatter,
                "energy-store disassembly overflows stockpile {} mass accounting",
                stockpile.value()
            ),
            Self::DestinationCapacityExceeded {
                stockpile,
                capacity,
                committed,
                requested,
            } => write!(
                formatter,
                "energy-store disassembly exceeds stockpile {} capacity {} mg: {} mg committed, {} mg requested",
                stockpile.value(),
                capacity.milligrams(),
                committed.milligrams(),
                requested.milligrams()
            ),
            Self::LotIdExhausted => formatter
                .write_str("material lot identifier space is exhausted during store disassembly"),
            Self::InventoryRevisionExhausted => formatter
                .write_str("inventory revision space is exhausted during store disassembly"),
            Self::EnergyRevisionExhausted => {
                formatter.write_str("energy revision space is exhausted during store disassembly")
            }
            Self::StoredMatterLoad(error) => write!(
                formatter,
                "energy-store disassembly cannot update destination stored-matter load: {error}"
            ),
        }
    }
}

impl Error for EnergyStoreDisassemblyError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::DestinationStorage(error) => Some(error),
            Self::StoredMatterLoad(error) => Some(error),
            Self::UnknownStore { .. }
            | Self::NoEmbodiedMatter { .. }
            | Self::StoreNotEmpty { .. }
            | Self::StoreBusyProduction { .. }
            | Self::StoreBusyManualPower { .. }
            | Self::UnknownDestination { .. }
            | Self::InvalidEmbodiedMatter { .. }
            | Self::DestinationMassOverflow { .. }
            | Self::DestinationCapacityExceeded { .. }
            | Self::LotIdExhausted
            | Self::InventoryRevisionExhausted
            | Self::EnergyRevisionExhausted => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EnergyStoreDisassemblyCommitError {
    StaleInventory {
        expected: u64,
        actual: u64,
    },
    StaleEnergy {
        expected: u64,
        actual: u64,
    },
    UnknownStore {
        store: EnergyStoreId,
    },
    StoreChanged {
        store: EnergyStoreId,
    },
    StoreBusyProduction {
        store: EnergyStoreId,
        job: ProductionJobId,
    },
    StoreBusyManualPower {
        store: EnergyStoreId,
    },
    Structure(StructuralCommitError),
}

impl Display for EnergyStoreDisassemblyCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleInventory { expected, actual } => write!(
                formatter,
                "energy-store disassembly expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::StaleEnergy { expected, actual } => write!(
                formatter,
                "energy-store disassembly expected energy revision {expected} but current revision is {actual}"
            ),
            Self::UnknownStore { store } => write!(
                formatter,
                "energy store {} disappeared before disassembly commit",
                store.value()
            ),
            Self::StoreChanged { store } => write!(
                formatter,
                "energy store {} changed after disassembly validation",
                store.value()
            ),
            Self::StoreBusyProduction { store, job } => write!(
                formatter,
                "energy store {} became occupied by production job {} before disassembly commit",
                store.value(),
                job.value()
            ),
            Self::StoreBusyManualPower { store } => write!(
                formatter,
                "energy store {} became reserved by direct player-powered generation before disassembly commit",
                store.value()
            ),
            Self::Structure(error) => write!(
                formatter,
                "energy-store disassembly structure failed: {error}"
            ),
        }
    }
}

impl Error for EnergyStoreDisassemblyCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Structure(error) => Some(error),
            Self::StaleInventory { .. }
            | Self::StaleEnergy { .. }
            | Self::UnknownStore { .. }
            | Self::StoreChanged { .. }
            | Self::StoreBusyProduction { .. }
            | Self::StoreBusyManualPower { .. } => None,
        }
    }
}
