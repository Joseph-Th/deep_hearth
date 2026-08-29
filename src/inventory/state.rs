//! Owns persistent stockpiles, material lots, indexes, reservations, and synchronized mutations.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::core::time::SimulationTick;
use crate::material::CommodityKey;
use crate::structural::{StructuralElementId, apply_support_index_change};

mod lot_mutation;
mod records;

pub(super) use lot_mutation::{
    LotSlice, apply_aggregate_withdraw, apply_consume_lot_slice, apply_insert_or_merge_new_lot,
    get_stockpile_mut_or_panic,
};
#[cfg(any(test, feature = "test-gameplay"))]
pub(super) use lot_mutation::{
    LotStorageTransition, apply_aggregate_deposit, apply_move_full_lot, apply_split_lot,
};
pub(crate) use records::{
    AMBIENT_PRESERVATION_MULTIPLIER_PPM, MaterialStorageHistory, STORAGE_AGE_PARTS_PER_TICK,
    checked_consumed_material_mass,
};
pub use records::{
    ConsumedMaterialTrace, MaterialLotId, MaterialLotProfile, MaterialLotProvenance,
    MaterialLotRecord, StockpileEnclosureRecord, StockpileId, StockpileRecord,
    StockpileStorageProfile, StockpileStorageProfileError,
};

/// Runtime owner for stockpile records and their generated identifiers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InventoryState {
    revision: u64,
    next_stockpile_id: u32,
    next_lot_id: u64,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_btree_map_no_duplicates")]
    stockpiles: BTreeMap<StockpileId, StockpileRecord>,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_btree_map_no_duplicates")]
    lots: BTreeMap<MaterialLotId, MaterialLotRecord>,
    #[serde(skip)]
    lot_indexes: BTreeMap<StockpileId, StockpileLotIndex>,
    #[serde(skip)]
    stockpiles_by_support: BTreeMap<StructuralElementId, BTreeSet<StockpileId>>,
}

impl InventoryState {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            revision: 0,
            next_stockpile_id: 1,
            next_lot_id: 1,
            stockpiles: BTreeMap::new(),
            lots: BTreeMap::new(),
            lot_indexes: BTreeMap::new(),
            stockpiles_by_support: BTreeMap::new(),
        }
    }

    pub(crate) const fn revision(&self) -> u64 {
        self.revision
    }

    #[cfg(any(test, feature = "test-gameplay"))]
    pub(super) const fn next_stockpile_id(&self) -> u32 {
        self.next_stockpile_id
    }

    pub(super) const fn next_lot_id(&self) -> u64 {
        self.next_lot_id
    }

    #[cfg(any(test, feature = "test-gameplay"))]
    pub(super) fn insert_stockpile(
        &mut self,
        record: StockpileRecord,
        next_stockpile_id: u32,
        next_revision: u64,
    ) {
        let id = record.id;
        assert!(
            !self.stockpiles.contains_key(&id),
            "validated stockpile ID must be globally unique"
        );
        let previous = self.stockpiles.insert(id, record);
        assert!(
            previous.is_none(),
            "prechecked stockpile insertion unexpectedly replaced a record"
        );
        self.next_stockpile_id = next_stockpile_id;
        self.revision = next_revision;
    }

    pub(super) fn apply_lot_cursor_and_revision(&mut self, next_lot_id: u64, next_revision: u64) {
        self.next_lot_id = next_lot_id;
        self.revision = next_revision;
    }

    pub(super) fn apply_revision(&mut self, next_revision: u64) {
        self.revision = next_revision;
    }

    pub(super) fn apply_storage_enclosure(
        &mut self,
        stockpile: StockpileId,
        expected_profile: StockpileStorageProfile,
        next_profile: StockpileStorageProfile,
        enclosure: StockpileEnclosureRecord,
        at: SimulationTick,
        next_revision: u64,
    ) {
        assert_eq!(
            self.revision.checked_add(1),
            Some(next_revision),
            "validated storage construction must advance inventory revision exactly once after material egress"
        );
        let source_preservation = expected_profile.preservation_multiplier_ppm();
        for lot in self
            .lots
            .values_mut()
            .filter(|lot| lot.stockpile == stockpile)
        {
            lot.storage_history = lot
                .storage_history
                .rebase(at, source_preservation)
                .unwrap_or_else(|| {
                    panic!("validated storage construction overflowed lot storage history")
                });
        }
        let record = self.stockpiles.get_mut(&stockpile).unwrap_or_else(|| {
            panic!(
                "runtime invariant broken: stockpile {} disappeared during enclosure construction",
                stockpile.value()
            )
        });
        assert_eq!(
            record.storage_profile, expected_profile,
            "validated storage construction target profile changed before apply"
        );
        assert!(
            record.enclosure.is_none(),
            "validated storage construction target unexpectedly gained an enclosure"
        );
        record.storage_profile = next_profile;
        record.enclosure = Some(enclosure);
        self.revision = next_revision;
    }

    pub(crate) fn has_valid_id_cursors(&self) -> bool {
        self.next_stockpile_id != 0
            && self.next_lot_id != 0
            && self
                .stockpiles
                .keys()
                .next_back()
                .is_none_or(|highest| highest.value() < self.next_stockpile_id)
            && self
                .lots
                .keys()
                .next_back()
                .is_none_or(|highest| highest.value() < self.next_lot_id)
    }

    /// Returns one stockpile by stable runtime ID.
    #[must_use]
    pub fn get_stockpile(&self, id: StockpileId) -> Option<&StockpileRecord> {
        self.stockpiles.get(&id)
    }

    /// Iterates stockpiles deterministically by stable runtime ID.
    pub fn stockpiles(&self) -> impl Iterator<Item = &StockpileRecord> {
        self.stockpiles.values()
    }

    /// Returns one homogeneous material lot by stable runtime ID.
    #[must_use]
    pub fn get_lot(&self, id: MaterialLotId) -> Option<&MaterialLotRecord> {
        self.lots.get(&id)
    }

    /// Iterates all material lots deterministically by stable runtime ID.
    pub fn lots(&self) -> impl Iterator<Item = &MaterialLotRecord> {
        self.lots.values()
    }

    /// Iterates one stockpile's owned lots in stable persistent-ID order.
    pub fn lot_ids(&self, stockpile: StockpileId) -> impl Iterator<Item = MaterialLotId> + '_ {
        self.lot_indexes
            .get(&stockpile)
            .into_iter()
            .flat_map(StockpileLotIndex::lot_ids)
    }

    pub(super) fn lot_ids_for_commodity(
        &self,
        stockpile: StockpileId,
        commodity: CommodityKey,
    ) -> impl Iterator<Item = MaterialLotId> + '_ {
        self.lot_indexes
            .get(&stockpile)
            .into_iter()
            .flat_map(move |index| index.lot_ids_for_commodity(commodity))
    }

    pub(super) fn insert_lot_index(
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

    pub(super) fn remove_lot_index(
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
        for stockpile in self.stockpiles.values() {
            if let Some(support) = stockpile.supported_by {
                stockpiles_by_support
                    .entry(support)
                    .or_default()
                    .insert(stockpile.id);
            }
        }
        for lot in self.lots.values() {
            if !self.stockpiles.contains_key(&lot.stockpile) {
                continue;
            }
            lot_indexes
                .entry(lot.stockpile)
                .or_default()
                .insert(lot.id, lot.commodity());
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

    pub(super) fn apply_support_change(
        &mut self,
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
        apply_support_index_change(&mut self.stockpiles_by_support, stockpile, before, after);
        let record = match self.stockpiles.get_mut(&stockpile) {
            Some(record) => record,
            None => unreachable!("stockpile support record was prechecked before index mutation"),
        };
        record.supported_by = after;
        self.revision = next_revision;
    }
}

mod lot_index;
mod validation;

use lot_index::StockpileLotIndex;
pub use validation::InventoryValidationError;
pub(crate) use validation::validate_loaded_inventory;
