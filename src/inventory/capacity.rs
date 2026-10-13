//! Revision-bound stockpile capacity changes owned by inventory.

use crate::core::quantity::Mass;
use crate::core::state::AppState;

use super::{InventoryState, StockpileId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StockpileCapacityChangeError {
    UnknownStockpile {
        stockpile: StockpileId,
    },
    ZeroCapacity {
        stockpile: StockpileId,
    },
    CapacityBelowCommitted {
        stockpile: StockpileId,
        capacity: Mass,
        committed: Mass,
    },
    RevisionExhausted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StockpileCapacityChangeCommitError {
    StaleInventoryRevision {
        expected: u64,
        actual: u64,
    },
    CapacityChanged {
        stockpile: StockpileId,
        expected: Mass,
        actual: Mass,
    },
}

#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ValidatedStockpileCapacityChange {
    expected_revision: u64,
    next_revision: u64,
    stockpile: StockpileId,
    expected_capacity: Mass,
    next_capacity: Mass,
}

impl ValidatedStockpileCapacityChange {
    pub(crate) fn precheck(
        self,
        state: &AppState,
    ) -> Result<(), StockpileCapacityChangeCommitError> {
        let actual_revision = state.inventory().revision();
        if actual_revision != self.expected_revision {
            return Err(StockpileCapacityChangeCommitError::StaleInventoryRevision {
                expected: self.expected_revision,
                actual: actual_revision,
            });
        }
        let actual_capacity = state
            .inventory()
            .get_stockpile(self.stockpile)
            .map(|record| record.capacity())
            .unwrap_or_else(|| {
                unreachable!("validated stockpile capacity target disappeared before commit")
            });
        if actual_capacity != self.expected_capacity {
            return Err(StockpileCapacityChangeCommitError::CapacityChanged {
                stockpile: self.stockpile,
                expected: self.expected_capacity,
                actual: actual_capacity,
            });
        }
        Ok(())
    }

    pub(crate) fn apply(self, state: &mut AppState) {
        state.inventory_state_mut().apply_stockpile_capacity_change(
            self.stockpile,
            self.expected_capacity,
            self.next_capacity,
            self.next_revision,
        );
    }
}

pub(crate) fn validate_stockpile_capacity_change(
    state: &InventoryState,
    stockpile: StockpileId,
    next_capacity: Mass,
) -> Result<ValidatedStockpileCapacityChange, StockpileCapacityChangeError> {
    let record = state
        .get_stockpile(stockpile)
        .ok_or(StockpileCapacityChangeError::UnknownStockpile { stockpile })?;
    if next_capacity.is_zero() {
        return Err(StockpileCapacityChangeError::ZeroCapacity { stockpile });
    }
    let committed = record
        .stored_mass()
        .checked_add(record.reserved_inbound())
        .unwrap_or_else(|| panic!("validated inventory stockpile committed mass overflowed"));
    if committed > next_capacity {
        return Err(StockpileCapacityChangeError::CapacityBelowCommitted {
            stockpile,
            capacity: next_capacity,
            committed,
        });
    }
    let expected_revision = state.revision();
    let next_revision = expected_revision
        .checked_add(1)
        .ok_or(StockpileCapacityChangeError::RevisionExhausted)?;
    Ok(ValidatedStockpileCapacityChange {
        expected_revision,
        next_revision,
        stockpile,
        expected_capacity: record.capacity(),
        next_capacity,
    })
}
