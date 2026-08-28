//! Inventory allocator and derived-index validation.

use std::collections::BTreeMap;

use crate::structural::{SupportIndexValidationFault, validate_support_index};

use super::super::{InventoryState, StockpileId, StockpileLotIndex};
use super::InventoryValidationError;

pub(super) fn validate_inventory_cursors(
    state: &InventoryState,
) -> Result<(), InventoryValidationError> {
    validate_stockpile_cursor(state)?;
    validate_lot_cursor(state)
}

fn validate_stockpile_cursor(state: &InventoryState) -> Result<(), InventoryValidationError> {
    if state.next_stockpile_id == 0 {
        return Err(InventoryValidationError::ZeroNextStockpileId);
    }
    if let Some(highest) = state.stockpiles.keys().next_back().copied()
        && state.next_stockpile_id <= highest.value()
    {
        return Err(InventoryValidationError::NextIdNotAfterExisting {
            next: state.next_stockpile_id,
            highest,
        });
    }
    Ok(())
}

fn validate_lot_cursor(state: &InventoryState) -> Result<(), InventoryValidationError> {
    if state.next_lot_id == 0 {
        return Err(InventoryValidationError::ZeroNextLotId);
    }
    if let Some(highest) = state.lots.keys().next_back().copied()
        && state.next_lot_id <= highest.value()
    {
        return Err(InventoryValidationError::NextLotIdNotAfterExisting {
            next: state.next_lot_id,
            highest,
        });
    }
    Ok(())
}

pub(super) fn validate_lot_indexes(
    state: &InventoryState,
    expected_lot_indexes: BTreeMap<StockpileId, StockpileLotIndex>,
) -> Result<(), InventoryValidationError> {
    if state.lot_indexes != expected_lot_indexes {
        let stockpile = match state
            .lot_indexes
            .keys()
            .chain(expected_lot_indexes.keys())
            .find(|stockpile| {
                state.lot_indexes.get(stockpile) != expected_lot_indexes.get(stockpile)
            })
            .copied()
        {
            Some(stockpile) => stockpile,
            None => panic!("unequal lot-index maps must have a differing key"),
        };
        return Err(InventoryValidationError::LotIndexMismatch { stockpile });
    }
    Ok(())
}

pub(super) fn validate_stockpile_support_index(
    state: &InventoryState,
) -> Result<(), InventoryValidationError> {
    validate_support_index(
        &state.stockpiles_by_support,
        |_stockpile| false,
        |stockpile| {
            state
                .stockpiles
                .get(&stockpile)
                .map(|record| record.supported_by)
        },
    )
    .map_err(|fault| match fault {
        SupportIndexValidationFault::ZeroSupportElementId => {
            InventoryValidationError::ZeroIndexedSupportElementId
        }
        SupportIndexValidationFault::EmptySupportBucket { element } => {
            InventoryValidationError::EmptySupportIndex { element }
        }
        SupportIndexValidationFault::InvalidItemId { .. } => {
            unreachable!("stockpile support validation does not define an additional ID rule")
        }
        SupportIndexValidationFault::UnknownIndexedItem { item, element } => {
            InventoryValidationError::UnknownIndexedStockpile {
                stockpile: item,
                element,
            }
        }
        SupportIndexValidationFault::SupportMismatch {
            item,
            indexed,
            actual,
        } => InventoryValidationError::SupportIndexMismatch {
            stockpile: item,
            indexed,
            actual,
        },
    })
}
