//! Validates persisted inventory records, cached totals, indexes, storage policy, and references.

use std::collections::BTreeMap;

use crate::core::quantity::Mass;
use crate::core::time::SimulationTick;
use crate::material::{CommodityKey, MaterialRegistry};

use super::{InventoryState, MaterialLotRecord, StockpileId, StockpileLotIndex};

mod error;
mod indexes;
mod lot;
mod stockpile;

pub use error::InventoryValidationError;
use indexes::{validate_inventory_cursors, validate_lot_indexes, validate_stockpile_support_index};
use lot::validate_material_lot;
use stockpile::{validate_stockpile_record, validate_storage_profiles};

#[derive(Default)]
struct StockpileLotTotals {
    total: Mass,
    by_commodity: BTreeMap<CommodityKey, Mass>,
}

impl StockpileLotTotals {
    fn add_lot(&mut self, lot: &MaterialLotRecord) -> Result<(), InventoryValidationError> {
        self.total =
            self.total
                .checked_add(lot.mass)
                .ok_or(InventoryValidationError::MassOverflow {
                    stockpile: lot.stockpile,
                })?;
        let commodity = lot.commodity();
        let current = self
            .by_commodity
            .get(&commodity)
            .copied()
            .unwrap_or(Mass::ZERO);
        let next = current
            .checked_add(lot.mass)
            .ok_or(InventoryValidationError::MassOverflow {
                stockpile: lot.stockpile,
            })?;
        self.by_commodity.insert(commodity, next);
        Ok(())
    }
}

pub(crate) fn validate_loaded_inventory(
    materials: &MaterialRegistry,
    state: &InventoryState,
    current_tick: SimulationTick,
) -> Result<(), InventoryValidationError> {
    validate_inventory_cursors(state)?;
    validate_storage_profiles(state)?;

    let mut expected_lot_indexes = BTreeMap::<StockpileId, StockpileLotIndex>::new();
    let mut calculated_by_stockpile = BTreeMap::<StockpileId, StockpileLotTotals>::new();
    for (key, lot) in &state.lots {
        validate_material_lot(materials, state, *key, lot, current_tick)?;
        expected_lot_indexes
            .entry(lot.stockpile)
            .or_default()
            .insert(*key, lot.commodity());
        calculated_by_stockpile
            .entry(lot.stockpile)
            .or_default()
            .add_lot(lot)?;
    }

    for (key, record) in &state.stockpiles {
        let calculated = calculated_by_stockpile.remove(key).unwrap_or_default();
        validate_stockpile_record(state, *key, record, calculated)?;
    }
    validate_lot_indexes(state, expected_lot_indexes)?;
    validate_stockpile_support_index(state)?;
    debug_assert!(calculated_by_stockpile.is_empty());
    Ok(())
}
