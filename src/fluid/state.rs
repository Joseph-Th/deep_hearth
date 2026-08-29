//! Owns persistent finite fluid stores, contents, support assignment, and identity/revision state.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::core::quantity::{Temperature, Volume};
use crate::core::time::SimulationTick;
use crate::structural::{StructuralElementId, apply_support_index_change};

use super::definitions::FluidDefinitionId;

/// Persistent identity of one finite runtime fluid store.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FluidStoreId(u64);

impl FluidStoreId {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        assert!(value != 0, "fluid store id must be nonzero");
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}

/// Homogeneous contents of one finite store.
///
/// Temperature is retained because merging different thermal states without a heat-balance
/// resolver would create or destroy modeled sensible heat. Empty stores use `None` rather than a
/// zero-volume fluid identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FluidContents {
    pub(super) fluid: FluidDefinitionId,
    pub(super) volume: Volume,
    pub(super) temperature: Temperature,
}

impl FluidContents {
    #[must_use]
    pub const fn fluid(self) -> FluidDefinitionId {
        self.fluid
    }

    #[must_use]
    pub const fn volume(self) -> Volume {
        self.volume
    }

    #[must_use]
    pub const fn temperature(self) -> Temperature {
        self.temperature
    }
}

/// Authoritative runtime state for one finite fluid store.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FluidStoreRecord {
    pub(super) id: FluidStoreId,
    pub(super) capacity: Volume,
    pub(super) contents: Option<FluidContents>,
    pub(super) supported_by: Option<StructuralElementId>,
    pub(super) created_at: SimulationTick,
}

impl FluidStoreRecord {
    #[must_use]
    pub const fn id(&self) -> FluidStoreId {
        self.id
    }

    #[must_use]
    pub const fn capacity(&self) -> Volume {
        self.capacity
    }

    #[must_use]
    pub const fn contents(&self) -> Option<FluidContents> {
        self.contents
    }

    #[must_use]
    pub const fn stored_volume(&self) -> Volume {
        match self.contents {
            Some(contents) => contents.volume,
            None => Volume::ZERO,
        }
    }

    /// Returns the structural member carrying this store's fluid weight, if assigned.
    #[must_use]
    pub const fn supported_by(&self) -> Option<StructuralElementId> {
        self.supported_by
    }

    #[must_use]
    pub const fn created_at(&self) -> SimulationTick {
        self.created_at
    }
}

/// Persistent owner for finite fluid stores and monotonic identity/revision cursors.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FluidState {
    revision: u64,
    next_store_id: u64,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_btree_map_no_duplicates")]
    records: BTreeMap<FluidStoreId, FluidStoreRecord>,
    #[serde(skip)]
    stores_by_support: BTreeMap<StructuralElementId, BTreeSet<FluidStoreId>>,
}

impl FluidState {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            revision: 0,
            next_store_id: 1,
            records: BTreeMap::new(),
            stores_by_support: BTreeMap::new(),
        }
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[cfg(any(test, feature = "test-gameplay"))]
    #[must_use]
    pub(super) const fn next_store_id(&self) -> u64 {
        self.next_store_id
    }

    #[must_use]
    pub fn get_store(&self, id: FluidStoreId) -> Option<&FluidStoreRecord> {
        self.records.get(&id)
    }

    pub fn stores(&self) -> impl Iterator<Item = &FluidStoreRecord> {
        self.records.values()
    }

    pub(crate) fn rebuild_derived_indexes(&mut self) {
        let mut stores_by_support = BTreeMap::<StructuralElementId, BTreeSet<FluidStoreId>>::new();
        for record in self.records.values() {
            if let Some(support) = record.supported_by {
                stores_by_support
                    .entry(support)
                    .or_default()
                    .insert(record.id);
            }
        }
        self.stores_by_support = stores_by_support;
    }

    /// Atomically inserts one fixture-allocated store and advances identity and revision cursors.
    #[cfg(any(test, feature = "test-gameplay"))]
    pub(super) fn insert_store(
        &mut self,
        record: FluidStoreRecord,
        next_store_id: u64,
        next_revision: u64,
    ) {
        assert!(
            !self.records.contains_key(&record.id),
            "Runtime Invariant 4 (Index Uniqueness): fluid store allocation replaced an existing record"
        );
        let previous = self.records.insert(record.id, record);
        assert!(
            previous.is_none(),
            "prechecked fluid store insertion unexpectedly replaced a record"
        );
        self.next_store_id = next_store_id;
        self.revision = next_revision;
    }

    /// Applies one validated egress final contents under one owner revision advance.
    pub(super) fn apply_egress_contents(
        &mut self,
        store: FluidStoreId,
        contents: Option<FluidContents>,
        next_revision: u64,
    ) {
        let Some(record) = self.records.get_mut(&store) else {
            unreachable!(
                "validated fluid egress source cannot disappear without a revision change"
            );
        };
        record.contents = contents;
        self.revision = next_revision;
    }

    /// Iterates fluid stores assigned to one structural support in stable store-ID order.
    pub(crate) fn supported_stores(
        &self,
        support: StructuralElementId,
    ) -> impl Iterator<Item = FluidStoreId> + '_ {
        self.stores_by_support
            .get(&support)
            .into_iter()
            .flat_map(|stores| stores.iter().copied())
    }

    pub(super) fn apply_support_change(
        &mut self,
        store: FluidStoreId,
        before: Option<StructuralElementId>,
        after: Option<StructuralElementId>,
        next_revision: u64,
    ) {
        assert_eq!(
            self.revision.checked_add(1),
            Some(next_revision),
            "validated fluid support change must advance the owner revision exactly once"
        );
        let record = match self.records.get(&store) {
            Some(record) => record,
            None => panic!(
                "runtime invariant broken: fluid store {} disappeared during support update",
                store.value()
            ),
        };
        assert_eq!(
            record.supported_by, before,
            "runtime invariant broken: fluid store support record disagrees with support index"
        );
        apply_support_index_change(&mut self.stores_by_support, store, before, after);
        let record = match self.records.get_mut(&store) {
            Some(record) => record,
            None => unreachable!("fluid support record was prechecked before index mutation"),
        };
        record.supported_by = after;
        self.revision = next_revision;
    }

    pub(crate) fn has_valid_id_cursor(&self) -> bool {
        self.next_store_id != 0
            && self
                .records
                .keys()
                .next_back()
                .is_none_or(|largest| largest.value() < self.next_store_id)
    }
}

mod validation;

pub use validation::FluidValidationError;
pub(crate) use validation::validate_loaded_fluid;
