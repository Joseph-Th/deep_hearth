//! Inventory-owned allocation of empty material-custody records.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::state::AppState;

use super::state::{InventoryState, StockpileId, StockpileRecord, StockpileStorageProfile};

/// Failure while validating creation of one empty capacity-constrained stockpile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EmptyStockpileAllocationError {
    ZeroCapacity,
    IdExhausted,
    RevisionExhausted,
}

impl Display for EmptyStockpileAllocationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroCapacity => formatter.write_str("stockpile capacity must be nonzero"),
            Self::IdExhausted => formatter.write_str("stockpile identifier space is exhausted"),
            Self::RevisionExhausted => formatter.write_str("inventory revision space is exhausted"),
        }
    }
}

impl Error for EmptyStockpileAllocationError {}

/// Failure when an already-validated stockpile allocation is committed against changed inventory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EmptyStockpileAllocationCommitError {
    StaleInventoryRevision { expected: u64, actual: u64 },
}

impl Display for EmptyStockpileAllocationCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleInventoryRevision { expected, actual } => write!(
                formatter,
                "validated stockpile allocation expected inventory revision {expected} but current revision is {actual}"
            ),
        }
    }
}

impl Error for EmptyStockpileAllocationCommitError {}

/// Revision-bound authorization to create one empty inventory-owned stockpile.
#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ValidatedEmptyStockpileAllocation {
    expected_revision: u64,
    next_revision: u64,
    id: StockpileId,
    next_id: u32,
    capacity: Mass,
    storage_profile: StockpileStorageProfile,
}

impl ValidatedEmptyStockpileAllocation {
    #[must_use]
    pub(crate) const fn stockpile(&self) -> StockpileId {
        self.id
    }

    pub(crate) fn commit(
        self,
        state: &mut AppState,
    ) -> Result<StockpileId, EmptyStockpileAllocationCommitError> {
        let actual = state.inventory().revision();
        if actual != self.expected_revision {
            return Err(
                EmptyStockpileAllocationCommitError::StaleInventoryRevision {
                    expected: self.expected_revision,
                    actual,
                },
            );
        }
        let record = StockpileRecord {
            id: self.id,
            capacity: self.capacity,
            storage_profile: self.storage_profile,
            enclosure: None,
            supported_by: None,
            stored_mass: Mass::ZERO,
            reserved_inbound: Mass::ZERO,
            contents: BTreeMap::new(),
        };
        state
            .inventory_state_mut()
            .insert_stockpile(record, self.next_id, self.next_revision);
        Ok(self.id)
    }
}

/// Validates one empty stockpile allocation without assigning world-space meaning to it.
///
/// Inventory owns capacity and identity only. A caller such as logistics must separately own why
/// the custody record exists and where it is physically located.
pub(crate) fn validate_empty_stockpile_allocation(
    state: &InventoryState,
    capacity: Mass,
    storage_profile: StockpileStorageProfile,
) -> Result<ValidatedEmptyStockpileAllocation, EmptyStockpileAllocationError> {
    if capacity.is_zero() {
        return Err(EmptyStockpileAllocationError::ZeroCapacity);
    }
    let id = StockpileId::new(state.next_stockpile_id());
    let next_id = state
        .next_stockpile_id()
        .checked_add(1)
        .ok_or(EmptyStockpileAllocationError::IdExhausted)?;
    let expected_revision = state.revision();
    let next_revision = expected_revision
        .checked_add(1)
        .ok_or(EmptyStockpileAllocationError::RevisionExhausted)?;
    Ok(ValidatedEmptyStockpileAllocation {
        expected_revision,
        next_revision,
        id,
        next_id,
        capacity,
        storage_profile,
    })
}

#[cfg(test)]
#[path = "allocation_tests.rs"]
mod tests;
