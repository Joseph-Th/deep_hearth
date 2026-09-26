//! Ground stockpile custody allocation.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::inventory::allocation::{
    EmptyStockpileAllocationCommitError, EmptyStockpileAllocationError,
    ValidatedEmptyStockpileAllocation, validate_empty_stockpile_allocation,
};
use crate::inventory::{StockpileId, StockpileStorageProfile};
use crate::spatial::VoxelCoord;

/// Failure while allocating one empty loose ground stockpile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroundStockpileAllocationError {
    ZeroCapacity,
    StockpileIdExhausted,
    InventoryRevisionExhausted,
    LogisticsRevisionExhausted,
}

impl Display for GroundStockpileAllocationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroCapacity => formatter.write_str("ground stockpile capacity must be nonzero"),
            Self::StockpileIdExhausted => {
                formatter.write_str("stockpile identifier space is exhausted")
            }
            Self::InventoryRevisionExhausted => {
                formatter.write_str("inventory revision space is exhausted")
            }
            Self::LogisticsRevisionExhausted => {
                formatter.write_str("logistics revision space is exhausted")
            }
        }
    }
}

impl Error for GroundStockpileAllocationError {}

/// Failure when an empty-ground allocation is committed against changed owner state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroundStockpileAllocationCommitError {
    StaleLogisticsRevision { expected: u64, actual: u64 },
    StaleInventoryRevision { expected: u64, actual: u64 },
}

impl Display for GroundStockpileAllocationCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleLogisticsRevision { expected, actual } => write!(
                formatter,
                "ground stockpile allocation expected logistics revision {expected} but current revision is {actual}"
            ),
            Self::StaleInventoryRevision { expected, actual } => write!(
                formatter,
                "ground stockpile allocation expected inventory revision {expected} but current revision is {actual}"
            ),
        }
    }
}

impl Error for GroundStockpileAllocationCommitError {}

/// Revision-bound creation of one empty inventory stockpile with a logistics-owned ground voxel.
#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub struct ValidatedGroundStockpileAllocation {
    expected_logistics_revision: u64,
    next_logistics_revision: u64,
    position: VoxelCoord,
    allocation: ValidatedEmptyStockpileAllocation,
}

impl ValidatedGroundStockpileAllocation {
    #[must_use]
    pub const fn stockpile(&self) -> StockpileId {
        self.allocation.stockpile()
    }

    pub fn commit(
        self,
        state: &mut AppState,
    ) -> Result<StockpileId, GroundStockpileAllocationCommitError> {
        let actual_logistics_revision = state.logistics().revision();
        if actual_logistics_revision != self.expected_logistics_revision {
            return Err(
                GroundStockpileAllocationCommitError::StaleLogisticsRevision {
                    expected: self.expected_logistics_revision,
                    actual: actual_logistics_revision,
                },
            );
        }
        let stockpile = self.allocation.commit(state).map_err(|error| match error {
            EmptyStockpileAllocationCommitError::StaleInventoryRevision { expected, actual } => {
                GroundStockpileAllocationCommitError::StaleInventoryRevision { expected, actual }
            }
        })?;
        state.logistics_state_mut().apply_ground_placement(
            self.expected_logistics_revision,
            self.next_logistics_revision,
            stockpile,
            self.position,
        );
        Ok(stockpile)
    }
}

/// Allocates one empty loose solid-matter stockpile at an explicitly authorized world voxel.
///
/// This creates custody and location only; it creates no matter and makes no claim about terrain,
/// resource generation, or how material arrives there. The caller supplies a finite capacity from
/// its own world/placement contract, while inventory remains authoritative for that capacity.
pub fn validate_allocate_ground_stockpile(
    state: &AppState,
    position: VoxelCoord,
    capacity: Mass,
) -> Result<ValidatedGroundStockpileAllocation, GroundStockpileAllocationError> {
    let allocation = validate_empty_stockpile_allocation(
        state.inventory(),
        capacity,
        StockpileStorageProfile::unbounded_solid_only(),
    )
    .map_err(|error| match error {
        EmptyStockpileAllocationError::ZeroCapacity => GroundStockpileAllocationError::ZeroCapacity,
        EmptyStockpileAllocationError::IdExhausted => {
            GroundStockpileAllocationError::StockpileIdExhausted
        }
        EmptyStockpileAllocationError::RevisionExhausted => {
            GroundStockpileAllocationError::InventoryRevisionExhausted
        }
    })?;
    let expected_logistics_revision = state.logistics().revision();
    let next_logistics_revision = expected_logistics_revision
        .checked_add(1)
        .ok_or(GroundStockpileAllocationError::LogisticsRevisionExhausted)?;
    Ok(ValidatedGroundStockpileAllocation {
        expected_logistics_revision,
        next_logistics_revision,
        position,
        allocation,
    })
}
