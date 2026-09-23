//! Derived inventory indexes and synchronized support/index mutations.

use std::collections::{BTreeMap, BTreeSet};

use crate::material::CommodityKey;
use crate::structural::{
    StructuralElementId, apply_support_index_change, assert_support_index_change_available,
};

use super::{InventoryState, MaterialLotId, StockpileId, StockpileLotIndex};

impl InventoryState {
    /// Iterates one stockpile's owned lots in stable persistent-ID order.
    pub fn lot_ids(&self, stockpile: StockpileId) -> impl Iterator<Item = MaterialLotId> + '_ {
        self.lot_indexes
            .get(&stockpile)
            .into_iter()
            .flat_map(StockpileLotIndex::lot_ids)
    }

    pub(in crate::inventory) fn lot_ids_for_commodity(
        &self,
        stockpile: StockpileId,
        commodity: CommodityKey,
    ) -> impl Iterator<Item = MaterialLotId> + '_ {
        self.lot_indexes
            .get(&stockpile)
            .into_iter()
            .flat_map(move |index| index.lot_ids_for_commodity(commodity))
    }

    pub(in crate::inventory) fn insert_lot_index(
        &mut self,
        stockpile: StockpileId,
        commodity: CommodityKey,
        lot: MaterialLotId,
    ) {
        self.lot_indexes
            .entry(stockpile)
            .or_default()
            .insert(lot, commodity);
    }

    pub(in crate::inventory) fn remove_lot_index(
        &mut self,
        stockpile: StockpileId,
        commodity: CommodityKey,
        lot: MaterialLotId,
    ) {
        let remove_entry = {
            let index = self
                .lot_indexes
                .get_mut(&stockpile)
                .unwrap_or_else(|| panic!("runtime invariant broken: missing stockpile lot index"));
            index.remove(lot, commodity);
            index.is_empty()
        };
        if remove_entry {
            self.lot_indexes.remove(&stockpile);
        }
    }

    pub(crate) fn rebuild_derived_indexes(&mut self) {
        let mut lot_indexes = BTreeMap::<StockpileId, StockpileLotIndex>::new();
        let mut stockpiles_by_support =
            BTreeMap::<StructuralElementId, BTreeSet<StockpileId>>::new();
        for (stockpile_id, stockpile) in &self.stockpiles {
            if let Some(support) = stockpile.supported_by {
                stockpiles_by_support
                    .entry(support)
                    .or_default()
                    .insert(*stockpile_id);
            }
        }
        for (lot_id, lot) in &self.lots {
            if !self.stockpiles.contains_key(&lot.stockpile) {
                continue;
            }
            lot_indexes
                .entry(lot.stockpile)
                .or_default()
                .insert(*lot_id, lot.commodity());
        }
        self.lot_indexes = lot_indexes;
        self.stockpiles_by_support = stockpiles_by_support;
    }

    /// Iterates stockpiles assigned to one structural support in stable stockpile-ID order.
    pub(crate) fn supported_stockpiles(
        &self,
        support: StructuralElementId,
    ) -> impl Iterator<Item = StockpileId> + '_ {
        self.stockpiles_by_support
            .get(&support)
            .into_iter()
            .flat_map(|stockpiles| stockpiles.iter().copied())
    }

    pub(in crate::inventory) fn assert_support_change_available(
        &self,
        stockpile: StockpileId,
        before: Option<StructuralElementId>,
        after: Option<StructuralElementId>,
        next_revision: u64,
    ) {
        assert_eq!(
            self.revision.checked_add(1),
            Some(next_revision),
            "validated stockpile support change must advance the owner revision exactly once"
        );
        let record = match self.stockpiles.get(&stockpile) {
            Some(record) => record,
            None => panic!(
                "runtime invariant broken: stockpile {} disappeared during support update",
                stockpile.value()
            ),
        };
        assert_eq!(
            record.supported_by, before,
            "runtime invariant broken: stockpile support record disagrees with support index"
        );
        assert_support_index_change_available(
            &self.stockpiles_by_support,
            stockpile,
            before,
            after,
        );
    }

    pub(in crate::inventory) fn apply_support_change(
        &mut self,
        stockpile: StockpileId,
        before: Option<StructuralElementId>,
        after: Option<StructuralElementId>,
        next_revision: u64,
    ) {
        self.assert_support_change_available(stockpile, before, after, next_revision);
        apply_support_index_change(&mut self.stockpiles_by_support, stockpile, before, after);
        let record = match self.stockpiles.get_mut(&stockpile) {
            Some(record) => record,
            None => unreachable!("stockpile support record was prechecked before index mutation"),
        };
        record.supported_by = after;
        self.revision = next_revision;
    }
}
