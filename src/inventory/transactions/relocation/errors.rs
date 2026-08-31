//! Diagnostics for exact material relocation validation and commit.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::structural::StructuralCommitError;

use super::super::super::state::StockpileId;
use super::super::super::storage_validation::StockpileStorageError;
use super::super::super::structural_integration::StockpileStructuralLoadError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MaterialRelocationError {
    StaleSelection {
        expected: u64,
        actual: u64,
    },
    UnknownSource {
        stockpile: StockpileId,
    },
    UnknownDestination {
        stockpile: StockpileId,
    },
    SameStockpile {
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
    LotIdExhausted,
    RevisionExhausted,
    StructuralLoad(StockpileStructuralLoadError),
}

impl Display for MaterialRelocationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleSelection { expected, actual } => write!(
                formatter,
                "exact material relocation expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::UnknownSource { stockpile } => write!(
                formatter,
                "exact material relocation source stockpile {} does not exist",
                stockpile.value()
            ),
            Self::UnknownDestination { stockpile } => write!(
                formatter,
                "exact material relocation destination stockpile {} does not exist",
                stockpile.value()
            ),
            Self::SameStockpile { stockpile } => write!(
                formatter,
                "exact material relocation requires distinct source and destination; both are stockpile {}",
                stockpile.value()
            ),
            Self::DestinationStorage(error) => write!(
                formatter,
                "exact material relocation destination rejects selected matter: {error}"
            ),
            Self::DestinationMassOverflow { stockpile } => write!(
                formatter,
                "exact material relocation overflows destination stockpile {} mass accounting",
                stockpile.value()
            ),
            Self::DestinationCapacityExceeded {
                stockpile,
                capacity,
                committed,
                requested,
            } => write!(
                formatter,
                "exact material relocation exceeds stockpile {} capacity {} mg: {} mg committed, {} mg requested",
                stockpile.value(),
                capacity.milligrams(),
                committed.milligrams(),
                requested.milligrams()
            ),
            Self::LotIdExhausted => formatter
                .write_str("material lot identifier space is exhausted during exact relocation"),
            Self::RevisionExhausted => {
                formatter.write_str("inventory revision space is exhausted during exact relocation")
            }
            Self::StructuralLoad(error) => write!(
                formatter,
                "exact material relocation structural load failed: {error}"
            ),
        }
    }
}

impl Error for MaterialRelocationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::DestinationStorage(error) => Some(error),
            Self::StructuralLoad(error) => Some(error),
            Self::StaleSelection { .. }
            | Self::UnknownSource { .. }
            | Self::UnknownDestination { .. }
            | Self::SameStockpile { .. }
            | Self::DestinationMassOverflow { .. }
            | Self::DestinationCapacityExceeded { .. }
            | Self::LotIdExhausted
            | Self::RevisionExhausted => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MaterialRelocationCommitError {
    StaleInventoryRevision { expected: u64, actual: u64 },
    Structure(StructuralCommitError),
}

impl Display for MaterialRelocationCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleInventoryRevision { expected, actual } => write!(
                formatter,
                "validated material relocation expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::Structure(error) => write!(
                formatter,
                "material relocation structural commit failed: {error}"
            ),
        }
    }
}

impl Error for MaterialRelocationCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Structure(error) => Some(error),
            Self::StaleInventoryRevision { .. } => None,
        }
    }
}
