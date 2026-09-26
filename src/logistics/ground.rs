//! World-space placement for loose ground stockpiles.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::inventory::StockpileId;
use crate::spatial::VoxelCoord;
use crate::structural::StructuralElementId;

mod allocation;
mod transfer;

pub use allocation::{
    GroundStockpileAllocationCommitError, GroundStockpileAllocationError,
    ValidatedGroundStockpileAllocation, validate_allocate_ground_stockpile,
};
pub use transfer::{
    GroundMaterialTransferCommitError, GroundMaterialTransferError,
    ValidatedGroundMaterialTransfer, validate_drop_to_ground, validate_pickup_from_ground,
};

/// Failure while binding an existing unsupported stockpile to one ground voxel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroundStockpilePlacementError {
    UnknownStockpile {
        stockpile: StockpileId,
    },
    PlayerCarried {
        stockpile: StockpileId,
    },
    AlreadyLocated {
        stockpile: StockpileId,
        position: VoxelCoord,
    },
    StructurallyMounted {
        stockpile: StockpileId,
        element: StructuralElementId,
    },
    ReservedInbound {
        stockpile: StockpileId,
        reserved: Mass,
    },
    BusyStorageDismantling {
        stockpile: StockpileId,
    },
    LogisticsRevisionExhausted,
}

impl Display for GroundStockpilePlacementError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownStockpile { stockpile } => {
                write!(formatter, "unknown ground stockpile {}", stockpile.value())
            }
            Self::PlayerCarried { stockpile } => write!(
                formatter,
                "stockpile {} is already carried by the player",
                stockpile.value()
            ),
            Self::AlreadyLocated {
                stockpile,
                position,
            } => write!(
                formatter,
                "stockpile {} is already located at voxel ({},{},{})",
                stockpile.value(),
                position.x(),
                position.y(),
                position.z()
            ),
            Self::StructurallyMounted { stockpile, element } => write!(
                formatter,
                "stockpile {} is mounted to structural element {} and cannot also be a loose ground stockpile",
                stockpile.value(),
                element.value()
            ),
            Self::ReservedInbound {
                stockpile,
                reserved,
            } => write!(
                formatter,
                "stockpile {} has {} mg of reserved inbound matter and cannot change world location",
                stockpile.value(),
                reserved.milligrams()
            ),
            Self::BusyStorageDismantling { stockpile } => write!(
                formatter,
                "stockpile {} participates in active storage dismantling and cannot change world location",
                stockpile.value()
            ),
            Self::LogisticsRevisionExhausted => {
                formatter.write_str("logistics revision space is exhausted")
            }
        }
    }
}

impl Error for GroundStockpilePlacementError {}

/// Failure when a validated ground placement no longer matches owner revisions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroundStockpilePlacementCommitError {
    StaleLogisticsRevision {
        expected: u64,
        actual: u64,
    },
    StaleInventorySupportRevision {
        expected: u64,
        actual: u64,
    },
    UnknownStockpile {
        stockpile: StockpileId,
    },
    PlayerCarried {
        stockpile: StockpileId,
    },
    StructurallyMounted {
        stockpile: StockpileId,
        element: StructuralElementId,
    },
    ReservedInbound {
        stockpile: StockpileId,
        reserved: Mass,
    },
    BusyStorageDismantling {
        stockpile: StockpileId,
    },
}

impl Display for GroundStockpilePlacementCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleLogisticsRevision { expected, actual } => write!(
                formatter,
                "ground placement expected logistics revision {expected} but current revision is {actual}"
            ),
            Self::StaleInventorySupportRevision { expected, actual } => write!(
                formatter,
                "ground placement expected inventory support revision {expected} but current revision is {actual}"
            ),
            Self::UnknownStockpile { stockpile } => write!(
                formatter,
                "ground placement stockpile {} disappeared before commit",
                stockpile.value()
            ),
            Self::PlayerCarried { stockpile } => write!(
                formatter,
                "ground placement stockpile {} became player-carried before commit",
                stockpile.value()
            ),
            Self::StructurallyMounted { stockpile, element } => write!(
                formatter,
                "ground placement stockpile {} became mounted to structural element {} before commit",
                stockpile.value(),
                element.value()
            ),
            Self::ReservedInbound {
                stockpile,
                reserved,
            } => write!(
                formatter,
                "ground placement stockpile {} gained {} mg of reserved inbound matter before commit",
                stockpile.value(),
                reserved.milligrams()
            ),
            Self::BusyStorageDismantling { stockpile } => write!(
                formatter,
                "ground placement stockpile {} entered active storage dismantling before commit",
                stockpile.value()
            ),
        }
    }
}

impl Error for GroundStockpilePlacementCommitError {}

/// Revision-bound placement of an existing loose stockpile into world space.
#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub struct ValidatedGroundStockpilePlacement {
    expected_logistics_revision: u64,
    next_logistics_revision: u64,
    expected_inventory_support_revision: u64,
    stockpile: StockpileId,
    position: VoxelCoord,
}

impl ValidatedGroundStockpilePlacement {
    pub fn commit(self, state: &mut AppState) -> Result<(), GroundStockpilePlacementCommitError> {
        let actual_logistics_revision = state.logistics().revision();
        if actual_logistics_revision != self.expected_logistics_revision {
            return Err(
                GroundStockpilePlacementCommitError::StaleLogisticsRevision {
                    expected: self.expected_logistics_revision,
                    actual: actual_logistics_revision,
                },
            );
        }
        let actual_support_revision = state.inventory().support_revision();
        if actual_support_revision != self.expected_inventory_support_revision {
            return Err(
                GroundStockpilePlacementCommitError::StaleInventorySupportRevision {
                    expected: self.expected_inventory_support_revision,
                    actual: actual_support_revision,
                },
            );
        }
        let record = state.inventory().get_stockpile(self.stockpile).ok_or(
            GroundStockpilePlacementCommitError::UnknownStockpile {
                stockpile: self.stockpile,
            },
        )?;
        if state
            .logistics()
            .player()
            .is_some_and(|player| player.carried_stockpile() == self.stockpile)
        {
            return Err(GroundStockpilePlacementCommitError::PlayerCarried {
                stockpile: self.stockpile,
            });
        }
        if let Some(element) = record.supported_by() {
            return Err(GroundStockpilePlacementCommitError::StructurallyMounted {
                stockpile: self.stockpile,
                element,
            });
        }
        if !record.reserved_inbound().is_zero() {
            return Err(GroundStockpilePlacementCommitError::ReservedInbound {
                stockpile: self.stockpile,
                reserved: record.reserved_inbound(),
            });
        }
        if state
            .player_work()
            .get_storage_dismantling_stockpile_occupant(self.stockpile)
            .is_some()
        {
            return Err(
                GroundStockpilePlacementCommitError::BusyStorageDismantling {
                    stockpile: self.stockpile,
                },
            );
        }
        state.logistics_state_mut().apply_ground_placement(
            self.expected_logistics_revision,
            self.next_logistics_revision,
            self.stockpile,
            self.position,
        );
        Ok(())
    }
}

/// Low-level world/bootstrap authorization for giving one unsupported stockpile a ground location.
///
/// Ordinary pickup/drop actions cannot call this to teleport matter; they only operate on stockpiles
/// already owned by this location map. World generation and placement adapters compose this boundary
/// only after separately proving why the stockpile exists at the requested voxel.
pub fn validate_place_ground_stockpile(
    state: &AppState,
    stockpile: StockpileId,
    position: VoxelCoord,
) -> Result<ValidatedGroundStockpilePlacement, GroundStockpilePlacementError> {
    let record = state
        .inventory()
        .get_stockpile(stockpile)
        .ok_or(GroundStockpilePlacementError::UnknownStockpile { stockpile })?;
    if state
        .logistics()
        .player()
        .is_some_and(|player| player.carried_stockpile() == stockpile)
    {
        return Err(GroundStockpilePlacementError::PlayerCarried { stockpile });
    }
    if let Some(existing) = state.logistics().stationary_stockpile_position(stockpile) {
        return Err(GroundStockpilePlacementError::AlreadyLocated {
            stockpile,
            position: existing,
        });
    }
    if let Some(element) = record.supported_by() {
        return Err(GroundStockpilePlacementError::StructurallyMounted { stockpile, element });
    }
    if !record.reserved_inbound().is_zero() {
        return Err(GroundStockpilePlacementError::ReservedInbound {
            stockpile,
            reserved: record.reserved_inbound(),
        });
    }
    if state
        .player_work()
        .get_storage_dismantling_stockpile_occupant(stockpile)
        .is_some()
    {
        return Err(GroundStockpilePlacementError::BusyStorageDismantling { stockpile });
    }
    let expected_logistics_revision = state.logistics().revision();
    let next_logistics_revision = expected_logistics_revision
        .checked_add(1)
        .ok_or(GroundStockpilePlacementError::LogisticsRevisionExhausted)?;
    Ok(ValidatedGroundStockpilePlacement {
        expected_logistics_revision,
        next_logistics_revision,
        expected_inventory_support_revision: state.inventory().support_revision(),
        stockpile,
        position,
    })
}
