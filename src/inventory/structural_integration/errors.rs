//! Public validation and commit errors for stockpile structural support transactions.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Force;
use crate::inventory::StockpileId;
use crate::production::{ProductionJobId, ProductionOccupancyRelease};
use crate::structural::{
    StructuralCommitError, StructuralElementId, StructuralLifecycle, StructuralMutationError,
};

/// Failure while deriving structure-owned load from stockpile matter ownership.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StockpileStructuralLoadError {
    UnknownStockpile {
        stockpile: StockpileId,
    },
    UnknownSupport {
        stockpile: StockpileId,
        element: StructuralElementId,
    },
    SupportNotActiveForIncrease {
        stockpile: StockpileId,
        element: StructuralElementId,
        lifecycle: StructuralLifecycle,
    },
    AggregateMassOverflow {
        element: StructuralElementId,
    },
    WeightForceOverflow {
        element: StructuralElementId,
    },
    ExistingLoadMismatch {
        element: StructuralElementId,
        stored: Force,
        expected: Force,
    },
    Structure(StructuralMutationError),
}

impl Display for StockpileStructuralLoadError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownStockpile { stockpile } => {
                write!(formatter, "unknown stockpile id {}", stockpile.value())
            }
            Self::UnknownSupport { stockpile, element } => write!(
                formatter,
                "stockpile {} references missing structural support {}",
                stockpile.value(),
                element.value()
            ),
            Self::SupportNotActiveForIncrease {
                stockpile,
                element,
                lifecycle,
            } => write!(
                formatter,
                "stockpile {} cannot add stored matter while structural support {} is {lifecycle:?}",
                stockpile.value(),
                element.value()
            ),
            Self::AggregateMassOverflow { element } => write!(
                formatter,
                "stored matter mass overflows aggregate accounting on structural element {}",
                element.value()
            ),
            Self::WeightForceOverflow { element } => write!(
                formatter,
                "stored matter weight exceeds structural force range on element {}",
                element.value()
            ),
            Self::ExistingLoadMismatch {
                element,
                stored,
                expected,
            } => write!(
                formatter,
                "structural element {} stores {} mN stored-matter load but inventory ownership requires {} mN",
                element.value(),
                stored.millinewtons(),
                expected.millinewtons()
            ),
            Self::Structure(error) => {
                write!(formatter, "stored-matter structural load failed: {error}")
            }
        }
    }
}

impl Error for StockpileStructuralLoadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Structure(error) => Some(error),
            Self::UnknownStockpile { .. }
            | Self::UnknownSupport { .. }
            | Self::SupportNotActiveForIncrease { .. }
            | Self::AggregateMassOverflow { .. }
            | Self::WeightForceOverflow { .. }
            | Self::ExistingLoadMismatch { .. } => None,
        }
    }
}

/// Failure while assigning or removing a stockpile's structural support.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StockpileSupportError {
    UnknownStockpile {
        stockpile: StockpileId,
    },
    AlreadyMounted {
        stockpile: StockpileId,
        element: StructuralElementId,
    },
    NotMounted {
        stockpile: StockpileId,
    },
    TargetNotActive {
        element: StructuralElementId,
        lifecycle: StructuralLifecycle,
    },
    StockpileBusy {
        stockpile: StockpileId,
        job: ProductionJobId,
        release: ProductionOccupancyRelease,
    },
    StockpileBusyStorageDismantling {
        stockpile: StockpileId,
    },
    InventoryRevisionExhausted,
    Load(StockpileStructuralLoadError),
}

impl Display for StockpileSupportError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownStockpile { stockpile } => {
                write!(formatter, "unknown stockpile id {}", stockpile.value())
            }
            Self::AlreadyMounted { stockpile, element } => write!(
                formatter,
                "stockpile {} is already supported by structural element {}",
                stockpile.value(),
                element.value()
            ),
            Self::NotMounted { stockpile } => write!(
                formatter,
                "stockpile {} has no structural support assignment to remove",
                stockpile.value()
            ),
            Self::TargetNotActive { element, lifecycle } => write!(
                formatter,
                "structural element {} is {lifecycle:?} and cannot receive a stockpile",
                element.value()
            ),
            Self::StockpileBusy {
                stockpile,
                job,
                release,
            } => write!(
                formatter,
                "stockpile {} is an in-flight output destination for production job {} {release} and cannot be moved",
                stockpile.value(),
                job.value()
            ),
            Self::StockpileBusyStorageDismantling { stockpile } => write!(
                formatter,
                "stockpile {} participates in active storage-enclosure dismantling and cannot be moved",
                stockpile.value()
            ),
            Self::InventoryRevisionExhausted => {
                formatter.write_str("inventory revision space is exhausted")
            }
            Self::Load(error) => write!(formatter, "stockpile support load failed: {error}"),
        }
    }
}

impl Error for StockpileSupportError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Load(error) => Some(error),
            Self::UnknownStockpile { .. }
            | Self::AlreadyMounted { .. }
            | Self::NotMounted { .. }
            | Self::TargetNotActive { .. }
            | Self::StockpileBusy { .. }
            | Self::StockpileBusyStorageDismantling { .. }
            | Self::InventoryRevisionExhausted => None,
        }
    }
}

/// Failure to commit a revision-bound stockpile support change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StockpileSupportCommitError {
    StaleInventoryRevision {
        expected: u64,
        actual: u64,
    },
    UnknownStockpile {
        stockpile: StockpileId,
    },
    SupportChanged {
        stockpile: StockpileId,
        expected: Option<StructuralElementId>,
        actual: Option<StructuralElementId>,
    },
    StockpileBusy {
        stockpile: StockpileId,
        job: ProductionJobId,
        release: ProductionOccupancyRelease,
    },
    StockpileBusyStorageDismantling {
        stockpile: StockpileId,
    },
    Structure(StructuralCommitError),
}

impl Display for StockpileSupportCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleInventoryRevision { expected, actual } => write!(
                formatter,
                "validated stockpile support change expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::UnknownStockpile { stockpile } => write!(
                formatter,
                "stockpile {} disappeared before support commit",
                stockpile.value()
            ),
            Self::SupportChanged {
                stockpile,
                expected,
                actual,
            } => write!(
                formatter,
                "stockpile {} support changed from expected {expected:?} to {actual:?} before commit",
                stockpile.value()
            ),
            Self::StockpileBusy {
                stockpile,
                job,
                release,
            } => write!(
                formatter,
                "stockpile {} became an in-flight output destination for production job {} {release} before support commit",
                stockpile.value(),
                job.value()
            ),
            Self::StockpileBusyStorageDismantling { stockpile } => write!(
                formatter,
                "stockpile {} became occupied by active storage-enclosure dismantling before support commit",
                stockpile.value()
            ),
            Self::Structure(error) => write!(
                formatter,
                "stockpile support structural commit failed: {error}"
            ),
        }
    }
}

impl Error for StockpileSupportCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Structure(error) => Some(error),
            Self::StaleInventoryRevision { .. }
            | Self::UnknownStockpile { .. }
            | Self::SupportChanged { .. }
            | Self::StockpileBusy { .. }
            | Self::StockpileBusyStorageDismantling { .. } => None,
        }
    }
}
