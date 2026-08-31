//! Diagnostics for energy-store upgrade validation and commit conflicts.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Energy, Mass};
use crate::inventory::{StockpileId, StockpileStructuralLoadError};
use crate::production::{ProductionJobId, ProductionOccupancyRelease};
use crate::structural::StructuralCommitError;

use super::super::{EnergyStoreDefinitionId, EnergyStoreId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EnergyStoreUpgradeError {
    UnknownStore {
        store: EnergyStoreId,
    },
    UnknownTargetDefinition {
        target: EnergyStoreDefinitionId,
    },
    NoUpgradeProfile {
        target: EnergyStoreDefinitionId,
    },
    WrongBaseDefinition {
        store: EnergyStoreId,
        required: EnergyStoreDefinitionId,
        actual: EnergyStoreDefinitionId,
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
    UnknownSource {
        stockpile: StockpileId,
    },
    InsufficientMaterial {
        stockpile: StockpileId,
        available: Mass,
        required: Mass,
    },
    SourceMassOverflow {
        stockpile: StockpileId,
    },
    StaleInventorySelection {
        expected: u64,
        actual: u64,
    },
    InventoryRevisionExhausted,
    EnergyRevisionExhausted,
    StructuralLoad(StockpileStructuralLoadError),
}

impl Display for EnergyStoreUpgradeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownStore { store } => {
                write!(formatter, "unknown energy store {}", store.value())
            }
            Self::UnknownTargetDefinition { target } => write!(
                formatter,
                "unknown target energy-store definition {}",
                target.value()
            ),
            Self::NoUpgradeProfile { target } => write!(
                formatter,
                "energy-store definition {} has no authored additive upgrade path",
                target.value()
            ),
            Self::WrongBaseDefinition {
                store,
                required,
                actual,
            } => write!(
                formatter,
                "energy store {} uses definition {} but upgrade requires base definition {}",
                store.value(),
                actual.value(),
                required.value()
            ),
            Self::StoreNotEmpty { store, stored } => write!(
                formatter,
                "energy store {} still owns {} nJ and must be empty before its physical storage body can be upgraded",
                store.value(),
                stored.nanojoules()
            ),
            Self::StoreBusyProduction {
                store,
                job,
                release,
            } => write!(
                formatter,
                "energy store {} is occupied by production job {} {release} and cannot be upgraded",
                store.value(),
                job.value()
            ),
            Self::StoreBusyManualPower { store } => write!(
                formatter,
                "energy store {} is reserved by direct player-powered generation and cannot be upgraded",
                store.value()
            ),
            Self::UnknownSource { stockpile } => write!(
                formatter,
                "unknown energy-store upgrade material stockpile {}",
                stockpile.value()
            ),
            Self::InsufficientMaterial {
                stockpile,
                available,
                required,
            } => write!(
                formatter,
                "energy-store upgrade stockpile {} contains {} mg but {} mg of authored addition material is required",
                stockpile.value(),
                available.milligrams(),
                required.milligrams()
            ),
            Self::SourceMassOverflow { stockpile } => write!(
                formatter,
                "energy-store upgrade source {} mass accounting overflowed",
                stockpile.value()
            ),
            Self::StaleInventorySelection { expected, actual } => write!(
                formatter,
                "energy-store upgrade material selection expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::InventoryRevisionExhausted => {
                formatter.write_str("inventory revision space is exhausted")
            }
            Self::EnergyRevisionExhausted => {
                formatter.write_str("energy revision space is exhausted")
            }
            Self::StructuralLoad(error) => write!(
                formatter,
                "energy-store upgrade source load failed: {error}"
            ),
        }
    }
}

impl Error for EnergyStoreUpgradeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::StructuralLoad(error) => Some(error),
            Self::UnknownStore { .. }
            | Self::UnknownTargetDefinition { .. }
            | Self::NoUpgradeProfile { .. }
            | Self::WrongBaseDefinition { .. }
            | Self::StoreNotEmpty { .. }
            | Self::StoreBusyProduction { .. }
            | Self::StoreBusyManualPower { .. }
            | Self::UnknownSource { .. }
            | Self::InsufficientMaterial { .. }
            | Self::SourceMassOverflow { .. }
            | Self::StaleInventorySelection { .. }
            | Self::InventoryRevisionExhausted
            | Self::EnergyRevisionExhausted => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EnergyStoreUpgradeCommitError {
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

impl Display for EnergyStoreUpgradeCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleInventory { expected, actual } => write!(
                formatter,
                "energy-store upgrade expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::StaleEnergy { expected, actual } => write!(
                formatter,
                "energy-store upgrade expected energy revision {expected} but current revision is {actual}"
            ),
            Self::UnknownStore { store } => write!(
                formatter,
                "energy store {} disappeared before upgrade commit",
                store.value()
            ),
            Self::StoreChanged { store } => write!(
                formatter,
                "energy store {} changed after upgrade validation",
                store.value()
            ),
            Self::StoreBusyProduction { store, job } => write!(
                formatter,
                "energy store {} became occupied by production job {} before upgrade commit",
                store.value(),
                job.value()
            ),
            Self::StoreBusyManualPower { store } => write!(
                formatter,
                "energy store {} became reserved by direct player-powered generation before upgrade commit",
                store.value()
            ),
            Self::Structure(error) => {
                write!(formatter, "energy-store upgrade structure failed: {error}")
            }
        }
    }
}

impl Error for EnergyStoreUpgradeCommitError {
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
