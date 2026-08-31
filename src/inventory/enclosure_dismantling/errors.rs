//! Diagnostics for exact stockpile enclosure recovery and commit.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::inventory::{
    MaterialLotId, StockpileId, StockpileStorageError, StockpileStructuralLoadError,
};
use crate::structural::{StructuralCommitError, StructuralElementId};

/// Failure while validating exact recovery of one stockpile enclosure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StorageEnclosureDismantleError {
    UnknownTarget {
        stockpile: StockpileId,
    },
    NotEnclosed {
        stockpile: StockpileId,
    },
    TargetMounted {
        stockpile: StockpileId,
        element: StructuralElementId,
    },
    TargetHasReservedInbound {
        stockpile: StockpileId,
        reserved: Mass,
    },
    UnknownRecoveryDestination {
        stockpile: StockpileId,
    },
    RecoveryDestinationIsTarget {
        stockpile: StockpileId,
    },
    TargetContentsIncompatible {
        lot: MaterialLotId,
        error: StockpileStorageError,
    },
    StorageHistoryOverflow {
        lot: MaterialLotId,
    },
    RecoveryDestinationStorage(StockpileStorageError),
    RecoveryCapacityExceeded {
        stockpile: StockpileId,
        capacity: Mass,
        committed: Mass,
        requested: Mass,
    },
    RecoveryMassOverflow {
        stockpile: StockpileId,
    },
    RecoveryLotIdExhausted,
    InventoryRevisionExhausted,
    StructuralLoad(StockpileStructuralLoadError),
}

impl Display for StorageEnclosureDismantleError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownTarget { stockpile } => write!(
                formatter,
                "unknown storage enclosure target stockpile {}",
                stockpile.value()
            ),
            Self::NotEnclosed { stockpile } => write!(
                formatter,
                "stockpile {} has no material-backed enclosure to dismantle",
                stockpile.value()
            ),
            Self::TargetMounted { stockpile, element } => write!(
                formatter,
                "stockpile {} must be unmounted before dismantling its enclosure; current support is {}",
                stockpile.value(),
                element.value()
            ),
            Self::TargetHasReservedInbound {
                stockpile,
                reserved,
            } => write!(
                formatter,
                "stockpile {} cannot change storage enclosure while {} mg of inbound matter is reserved",
                stockpile.value(),
                reserved.milligrams()
            ),
            Self::UnknownRecoveryDestination { stockpile } => write!(
                formatter,
                "unknown enclosure recovery destination stockpile {}",
                stockpile.value()
            ),
            Self::RecoveryDestinationIsTarget { stockpile } => write!(
                formatter,
                "stockpile {} cannot receive its own enclosure body during dismantling; use a distinct recovery stockpile",
                stockpile.value()
            ),
            Self::TargetContentsIncompatible { lot, error } => write!(
                formatter,
                "material lot {} cannot remain in ambient storage after enclosure dismantling: {error}",
                lot.value()
            ),
            Self::StorageHistoryOverflow { lot } => write!(
                formatter,
                "material lot {} cannot checkpoint its preserved storage exposure at dismantling time",
                lot.value()
            ),
            Self::RecoveryDestinationStorage(error) => {
                write!(
                    formatter,
                    "recovery destination rejects enclosure matter: {error}"
                )
            }
            Self::RecoveryCapacityExceeded {
                stockpile,
                capacity,
                committed,
                requested,
            } => write!(
                formatter,
                "stockpile {} capacity {} mg cannot accept {} mg of recovered enclosure matter after {} mg already committed",
                stockpile.value(),
                capacity.milligrams(),
                requested.milligrams(),
                committed.milligrams()
            ),
            Self::RecoveryMassOverflow { stockpile } => write!(
                formatter,
                "recovered enclosure matter overflows stockpile {} mass accounting",
                stockpile.value()
            ),
            Self::RecoveryLotIdExhausted => formatter
                .write_str("material lot identifier space is exhausted during enclosure recovery"),
            Self::InventoryRevisionExhausted => {
                formatter.write_str("inventory revision space is exhausted")
            }
            Self::StructuralLoad(error) => {
                write!(
                    formatter,
                    "recovered enclosure structural load failed: {error}"
                )
            }
        }
    }
}

impl Error for StorageEnclosureDismantleError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::RecoveryDestinationStorage(error) => Some(error),
            Self::TargetContentsIncompatible { error, .. } => Some(error),
            Self::StructuralLoad(error) => Some(error),
            Self::UnknownTarget { .. }
            | Self::NotEnclosed { .. }
            | Self::TargetMounted { .. }
            | Self::TargetHasReservedInbound { .. }
            | Self::UnknownRecoveryDestination { .. }
            | Self::RecoveryDestinationIsTarget { .. }
            | Self::StorageHistoryOverflow { .. }
            | Self::RecoveryCapacityExceeded { .. }
            | Self::RecoveryMassOverflow { .. }
            | Self::RecoveryLotIdExhausted
            | Self::InventoryRevisionExhausted => None,
        }
    }
}

/// Failure to commit an already validated enclosure dismantling transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StorageEnclosureDismantleCommitError {
    StaleInventoryRevision { expected: u64, actual: u64 },
    UnknownTarget { stockpile: StockpileId },
    TargetProfileChanged { stockpile: StockpileId },
    TargetEnclosureChanged { stockpile: StockpileId },
    Structure(StructuralCommitError),
}

impl Display for StorageEnclosureDismantleCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleInventoryRevision { expected, actual } => write!(
                formatter,
                "storage dismantling expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::UnknownTarget { stockpile } => write!(
                formatter,
                "storage dismantling target stockpile {} disappeared before commit",
                stockpile.value()
            ),
            Self::TargetProfileChanged { stockpile } => write!(
                formatter,
                "storage dismantling target stockpile {} changed storage profile before commit",
                stockpile.value()
            ),
            Self::TargetEnclosureChanged { stockpile } => write!(
                formatter,
                "storage dismantling target stockpile {} changed enclosure before commit",
                stockpile.value()
            ),
            Self::Structure(error) => write!(
                formatter,
                "storage dismantling structural-load commit failed: {error}"
            ),
        }
    }
}

impl Error for StorageEnclosureDismantleCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Structure(error) => Some(error),
            Self::StaleInventoryRevision { .. }
            | Self::UnknownTarget { .. }
            | Self::TargetProfileChanged { .. }
            | Self::TargetEnclosureChanged { .. } => None,
        }
    }
}
