//! Diagnostics for timed stockpile-enclosure dismantling admission.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::time::{SimulationTick, TickSpan};
use crate::inventory::{MaterialLotId, StockpileId, StockpileStorageError, StorageDefinitionId};
use crate::labor::{PlayerWorkCommitError, PlayerWorkStartError};
use crate::structural::StructuralElementId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StorageEnclosureDismantlingError {
    UnknownTarget {
        stockpile: StockpileId,
    },
    NotEnclosed {
        stockpile: StockpileId,
    },
    UnknownDefinition {
        definition: StorageDefinitionId,
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
    RecoveryDestinationMounted {
        stockpile: StockpileId,
        element: StructuralElementId,
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
    CompletionTickOverflow {
        current: SimulationTick,
        duration: TickSpan,
    },
    PlayerWork(PlayerWorkStartError),
}

impl Display for StorageEnclosureDismantlingError {
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
            Self::UnknownDefinition { definition } => write!(
                formatter,
                "unknown storage enclosure definition {}",
                definition.value()
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
                "stockpile {} cannot begin enclosure dismantling while {} mg of inbound matter is reserved",
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
            Self::RecoveryDestinationMounted { stockpile, element } => write!(
                formatter,
                "recovery stockpile {} must be unmounted while enclosure recovery is pending; current support is {}",
                stockpile.value(),
                element.value()
            ),
            Self::TargetContentsIncompatible { lot, error } => write!(
                formatter,
                "material lot {} cannot remain in ambient storage after enclosure dismantling: {error}",
                lot.value()
            ),
            Self::StorageHistoryOverflow { lot } => write!(
                formatter,
                "material lot {} cannot checkpoint its preserved storage exposure at dismantling completion",
                lot.value()
            ),
            Self::RecoveryDestinationStorage(error) => write!(
                formatter,
                "recovery destination rejects enclosure matter: {error}"
            ),
            Self::RecoveryCapacityExceeded {
                stockpile,
                capacity,
                committed,
                requested,
            } => write!(
                formatter,
                "stockpile {} capacity {} mg cannot reserve {} mg of recovered enclosure matter after {} mg already committed",
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
            Self::CompletionTickOverflow { current, duration } => write!(
                formatter,
                "storage dismantling starting at tick {} cannot schedule {} active ticks",
                current.value(),
                duration.value()
            ),
            Self::PlayerWork(error) => {
                write!(formatter, "storage dismantling labor cannot start: {error}")
            }
        }
    }
}

impl Error for StorageEnclosureDismantlingError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::RecoveryDestinationStorage(error)
            | Self::TargetContentsIncompatible { error, .. } => Some(error),
            Self::PlayerWork(error) => Some(error),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StorageEnclosureDismantlingCommitError {
    StaleInventoryRevision { expected: u64, actual: u64 },
    UnknownTarget { stockpile: StockpileId },
    TargetProfileChanged { stockpile: StockpileId },
    TargetEnclosureChanged { stockpile: StockpileId },
    PlayerWork(PlayerWorkCommitError),
}

impl Display for StorageEnclosureDismantlingCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleInventoryRevision { expected, actual } => write!(
                formatter,
                "storage dismantling admission expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::UnknownTarget { stockpile } => write!(
                formatter,
                "storage dismantling target stockpile {} disappeared before admission",
                stockpile.value()
            ),
            Self::TargetProfileChanged { stockpile } => write!(
                formatter,
                "storage dismantling target stockpile {} changed storage profile before admission",
                stockpile.value()
            ),
            Self::TargetEnclosureChanged { stockpile } => write!(
                formatter,
                "storage dismantling target stockpile {} changed enclosure before admission",
                stockpile.value()
            ),
            Self::PlayerWork(error) => write!(
                formatter,
                "storage dismantling labor commit failed: {error}"
            ),
        }
    }
}

impl Error for StorageEnclosureDismantlingCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::PlayerWork(error) => Some(error),
            _ => None,
        }
    }
}
