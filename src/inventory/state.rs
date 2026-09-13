//! Owns persistent stockpiles, material lots, indexes, reservations, and synchronized mutations.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::structural::StructuralElementId;

mod enclosure;
mod indexes;
mod lot_mutation;
mod records;
mod storage_history;

pub(super) use lot_mutation::{
    LotSlice, apply_aggregate_withdraw, apply_consume_lot_slice, apply_insert_or_merge_new_lot,
    get_stockpile_mut_or_panic,
};
#[cfg(any(test, feature = "test-gameplay"))]
pub(super) use lot_mutation::{
    LotStorageTransition, apply_aggregate_deposit, apply_move_full_lot, apply_split_lot,
};
pub(crate) use records::{
    AMBIENT_PRESERVATION_MULTIPLIER_PPM, PureMaterialTraceValidationError,
    checked_consumed_material_mass,
};
pub use records::{
    ConsumedMaterialTrace, MaterialLotId, MaterialLotProfile, MaterialLotProvenance,
    MaterialLotRecord, StockpileEnclosureRecord, StockpileId, StockpileRecord,
    StockpileStorageProfile, StockpileStorageProfileError,
};
pub(crate) use storage_history::{MaterialStorageHistory, STORAGE_AGE_PARTS_PER_TICK};

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
        assert_eq!(
            id.value(),
            self.next_stockpile_id,
            "stockpile allocation must consume the current identity cursor"
        );
        assert_eq!(
            self.next_stockpile_id.checked_add(1),
            Some(next_stockpile_id),
            "stockpile allocation must advance the identity cursor exactly once"
        );
        assert_eq!(
            self.revision.checked_add(1),
            Some(next_revision),
            "stockpile allocation must advance the owner revision exactly once"
        );
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
        assert_eq!(
            self.revision.checked_add(1),
            Some(next_revision),
            "inventory revision must advance exactly once per canonical lot mutation batch"
        );
        assert!(
            next_lot_id >= self.next_lot_id,
            "inventory lot cursor cannot move backward"
        );
        self.next_lot_id = next_lot_id;
        self.revision = next_revision;
    }

    pub(super) fn apply_revision(&mut self, next_revision: u64) {
        assert_eq!(
            self.revision.checked_add(1),
            Some(next_revision),
            "inventory revision must advance exactly once per canonical mutation batch"
        );
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
}

mod lot_index;
mod validation;

use lot_index::StockpileLotIndex;
pub use validation::InventoryValidationError;
pub(crate) use validation::validate_loaded_inventory;
