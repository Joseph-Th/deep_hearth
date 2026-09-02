//! Diagnostics for material-backed stockpile enclosure construction and commit.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::inventory::{
    MaterialLotId, StockpileId, StockpileStorageError, StockpileStorageProfile,
    StockpileStructuralLoadError, StorageDefinitionId,
};
use crate::material::CommodityKey;
use crate::structural::{StructuralCommitError, StructuralElementId};

/// Failure while validating construction of one authored storage enclosure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StorageEnclosureConstructionError {
    UnknownDefinition {
        definition: StorageDefinitionId,
    },
    UnknownTarget {
        stockpile: StockpileId,
    },
    UnknownSource {
        stockpile: StockpileId,
    },
    AlreadyEnclosed {
        stockpile: StockpileId,
        definition: StorageDefinitionId,
    },
    TargetMounted {
        stockpile: StockpileId,
        element: StructuralElementId,
    },
    TargetBusyStorageDismantling {
        stockpile: StockpileId,
    },
    TargetCapacityTooLarge {
        stockpile: StockpileId,
        capacity: Mass,
        maximum: Mass,
    },
    TargetStorageProfileMismatch {
        stockpile: StockpileId,
        current: StockpileStorageProfile,
        required: StockpileStorageProfile,
    },
    TargetHasReservedInbound {
        stockpile: StockpileId,
        reserved: Mass,
    },
    TargetContentsIncompatible {
        lot: MaterialLotId,
        error: StockpileStorageError,
    },
    StorageHistoryOverflow {
        lot: MaterialLotId,
    },
    InsufficientMaterial {
        stockpile: StockpileId,
        commodity: CommodityKey,
        available: Mass,
        required: Mass,
    },
    SourceMassOverflow {
        stockpile: StockpileId,
    },
    InventoryRevisionExhausted,
    StructuralLoad(StockpileStructuralLoadError),
}

impl Display for StorageEnclosureConstructionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownDefinition { definition } => write!(
                formatter,
                "unknown storage enclosure definition {}",
                definition.value()
            ),
            Self::UnknownTarget { stockpile } => write!(
                formatter,
                "unknown storage target stockpile {}",
                stockpile.value()
            ),
            Self::UnknownSource { stockpile } => write!(
                formatter,
                "unknown storage-construction material stockpile {}",
                stockpile.value()
            ),
            Self::AlreadyEnclosed {
                stockpile,
                definition,
            } => write!(
                formatter,
                "stockpile {} already has storage enclosure {}",
                stockpile.value(),
                definition.value()
            ),
            Self::TargetMounted { stockpile, element } => write!(
                formatter,
                "stockpile {} must be unmounted before constructing an enclosure around it; current support is {}",
                stockpile.value(),
                element.value()
            ),
            Self::TargetBusyStorageDismantling { stockpile } => write!(
                formatter,
                "stockpile {} participates in active storage-enclosure dismantling and cannot change enclosure",
                stockpile.value()
            ),
            Self::TargetCapacityTooLarge {
                stockpile,
                capacity,
                maximum,
            } => write!(
                formatter,
                "stockpile {} capacity {} mg exceeds enclosure maximum {} mg",
                stockpile.value(),
                capacity.milligrams(),
                maximum.milligrams()
            ),
            Self::TargetStorageProfileMismatch { stockpile, .. } => write!(
                formatter,
                "stockpile {} does not have the ambient solid-storage profile required for this enclosure",
                stockpile.value()
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
            Self::TargetContentsIncompatible { lot, error } => write!(
                formatter,
                "material lot {} is incompatible with the completed storage enclosure: {error}",
                lot.value()
            ),
            Self::StorageHistoryOverflow { lot } => write!(
                formatter,
                "material lot {} cannot checkpoint its existing storage exposure at construction time",
                lot.value()
            ),
            Self::InsufficientMaterial {
                stockpile,
                commodity,
                available,
                required,
            } => write!(
                formatter,
                "storage construction stockpile {} has {} mg of commodity {} but requires {} mg",
                stockpile.value(),
                available.milligrams(),
                commodity.value(),
                required.milligrams()
            ),
            Self::SourceMassOverflow { stockpile } => write!(
                formatter,
                "storage construction source stockpile {} mass accounting overflowed",
                stockpile.value()
            ),
            Self::InventoryRevisionExhausted => {
                formatter.write_str("inventory revision space is exhausted")
            }
            Self::StructuralLoad(error) => write!(
                formatter,
                "storage construction source-load update failed: {error}"
            ),
        }
    }
}

impl Error for StorageEnclosureConstructionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::TargetContentsIncompatible { error, .. } => Some(error),
            Self::StructuralLoad(error) => Some(error),
            Self::UnknownDefinition { .. }
            | Self::UnknownTarget { .. }
            | Self::UnknownSource { .. }
            | Self::AlreadyEnclosed { .. }
            | Self::TargetMounted { .. }
            | Self::TargetBusyStorageDismantling { .. }
            | Self::TargetCapacityTooLarge { .. }
            | Self::TargetStorageProfileMismatch { .. }
            | Self::TargetHasReservedInbound { .. }
            | Self::StorageHistoryOverflow { .. }
            | Self::InsufficientMaterial { .. }
            | Self::SourceMassOverflow { .. }
            | Self::InventoryRevisionExhausted => None,
        }
    }
}

/// Failure to commit a validated storage-enclosure construction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StorageEnclosureCommitError {
    StaleInventoryRevision { expected: u64, actual: u64 },
    UnknownTarget { stockpile: StockpileId },
    TargetProfileChanged { stockpile: StockpileId },
    TargetEnclosureChanged { stockpile: StockpileId },
    Structure(StructuralCommitError),
}

impl Display for StorageEnclosureCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleInventoryRevision { expected, actual } => write!(
                formatter,
                "storage construction expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::UnknownTarget { stockpile } => write!(
                formatter,
                "storage construction target stockpile {} disappeared before commit",
                stockpile.value()
            ),
            Self::TargetProfileChanged { stockpile } => write!(
                formatter,
                "storage construction target stockpile {} changed storage profile before commit",
                stockpile.value()
            ),
            Self::TargetEnclosureChanged { stockpile } => write!(
                formatter,
                "storage construction target stockpile {} gained an enclosure before commit",
                stockpile.value()
            ),
            Self::Structure(error) => write!(
                formatter,
                "storage construction structural-load commit failed: {error}"
            ),
        }
    }
}

impl Error for StorageEnclosureCommitError {
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
