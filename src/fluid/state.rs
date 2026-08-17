//! Persistent finite fluid-store records; child validation audits durable state and references.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::core::quantity::{Temperature, Volume};
use crate::core::time::SimulationTick;
use crate::structural::StructuralElementId;

use super::definitions::{FluidDefinitionId, FluidRegistry};

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
pub struct FluidState {
    revision: u64,
    next_store_id: u64,
    records: BTreeMap<FluidStoreId, FluidStoreRecord>,
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

    /// Atomically inserts one allocated store record and advances the identity and revision cursors.
    pub(super) fn insert_store(
        &mut self,
        record: FluidStoreRecord,
        next_store_id: u64,
        next_revision: u64,
    ) {
        let previous = self.records.insert(record.id, record);
        debug_assert!(
            previous.is_none(),
            "Runtime Invariant 4 (Index Uniqueness): fluid store allocation replaced an existing record"
        );
        self.next_store_id = next_store_id;
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
        if let Some(before) = before {
            let remove_entry = {
                let indexed = match self.stores_by_support.get_mut(&before) {
                    Some(indexed) => indexed,
                    None => panic!(
                        "runtime invariant broken: fluid support index missing element {} for store {}",
                        before.value(),
                        store.value()
                    ),
                };
                assert!(
                    indexed.remove(&store),
                    "runtime invariant broken: fluid support index element {} missing store {}",
                    before.value(),
                    store.value()
                );
                indexed.is_empty()
            };
            if remove_entry {
                self.stores_by_support.remove(&before);
            }
        }
        if let Some(after) = after {
            let inserted = self
                .stores_by_support
                .entry(after)
                .or_default()
                .insert(store);
            assert!(
                inserted,
                "runtime invariant broken: fluid support index element {} already contains store {}",
                after.value(),
                store.value()
            );
        }
        let record = match self.records.get_mut(&store) {
            Some(record) => record,
            None => panic!(
                "runtime invariant broken: fluid store {} disappeared during support update",
                store.value()
            ),
        };
        debug_assert_eq!(record.supported_by, before);
        record.supported_by = after;
        self.revision = next_revision;
    }

    /// Applies one validated transfer's final contents to both stores under one revision advance.
    pub(super) fn apply_transfer_contents(
        &mut self,
        source: FluidStoreId,
        source_contents: Option<FluidContents>,
        destination: FluidStoreId,
        destination_contents: FluidContents,
        next_revision: u64,
    ) {
        let Some(source_record) = self.records.get_mut(&source) else {
            unreachable!("validated fluid source cannot disappear without a revision change");
        };
        source_record.contents = source_contents;
        let Some(destination_record) = self.records.get_mut(&destination) else {
            unreachable!("validated fluid destination cannot disappear without a revision change");
        };
        destination_record.contents = Some(destination_contents);
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
