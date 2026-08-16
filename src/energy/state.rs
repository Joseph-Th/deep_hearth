//! Persistent finite-energy ownership; child validation audits immutable references and runtime invariants.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::core::quantity::Energy;
use crate::core::time::SimulationTick;

use super::definitions::{EnergyRegistry, EnergyStoreDefinitionId};

/// Persistent identity of one runtime energy store.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EnergyStoreId(u64);

impl EnergyStoreId {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        assert!(value != 0, "energy store id must be nonzero");
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}

/// Authoritative changing state for one finite energy store.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnergyStoreRecord {
    pub(super) id: EnergyStoreId,
    pub(super) definition: EnergyStoreDefinitionId,
    pub(super) stored: Energy,
    pub(super) created_at: SimulationTick,
}

impl EnergyStoreRecord {
    #[must_use]
    pub const fn id(&self) -> EnergyStoreId {
        self.id
    }

    #[must_use]
    pub const fn definition(&self) -> EnergyStoreDefinitionId {
        self.definition
    }

    #[must_use]
    pub const fn stored(&self) -> Energy {
        self.stored
    }

    #[must_use]
    pub const fn created_at(&self) -> SimulationTick {
        self.created_at
    }
}

/// Persistent owner for finite energy stores and their monotonic identity/revision cursors.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnergyState {
    revision: u64,
    next_store_id: u64,
    records: BTreeMap<EnergyStoreId, EnergyStoreRecord>,
}

impl EnergyState {
    pub(crate) const fn new() -> Self {
        Self {
            revision: 0,
            next_store_id: 1,
            records: BTreeMap::new(),
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
    pub fn get_store(&self, id: EnergyStoreId) -> Option<&EnergyStoreRecord> {
        self.records.get(&id)
    }

    pub fn stores(&self) -> impl Iterator<Item = &EnergyStoreRecord> {
        self.records.values()
    }

    pub(super) fn insert_store(
        &mut self,
        record: EnergyStoreRecord,
        next_store_id: u64,
        next_revision: u64,
    ) {
        let previous = self.records.insert(record.id, record);
        assert!(
            previous.is_none(),
            "Runtime Invariant 4 (Index Uniqueness): energy store allocation replaced an existing record"
        );
        self.next_store_id = next_store_id;
        self.revision = next_revision;
    }

    pub(super) fn add_stored_energy(&mut self, store: EnergyStoreId, energy: Energy) {
        let record = self.records.get_mut(&store).unwrap_or_else(|| {
            panic!(
                "runtime invariant broken: energy sink {} disappeared before commit",
                store.value()
            )
        });
        record.stored = record
            .stored
            .checked_add(energy)
            .unwrap_or_else(|| panic!("runtime invariant broken: energy sink overflowed"));
    }

    pub(super) fn subtract_stored_energy(&mut self, store: EnergyStoreId, energy: Energy) {
        let record = self.records.get_mut(&store).unwrap_or_else(|| {
            panic!(
                "runtime invariant broken: energy source {} disappeared before commit",
                store.value()
            )
        });
        record.stored = record
            .stored
            .checked_sub(energy)
            .unwrap_or_else(|| panic!("runtime invariant broken: prevalidated energy disappeared"));
    }

    pub(super) fn apply_transfer_contents(
        &mut self,
        source: EnergyStoreId,
        source_after: Energy,
        destination: EnergyStoreId,
        destination_after: Energy,
        next_revision: u64,
    ) {
        self.records
            .get_mut(&source)
            .unwrap_or_else(|| {
                panic!("prevalidated energy transfer source disappeared before commit")
            })
            .stored = source_after;
        self.records
            .get_mut(&destination)
            .unwrap_or_else(|| {
                panic!("prevalidated energy transfer destination disappeared before commit")
            })
            .stored = destination_after;
        self.revision = next_revision;
    }

    pub(super) fn apply_revision(&mut self, next_revision: u64) {
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

pub use validation::EnergyValidationError;
pub(crate) use validation::validate_loaded_energy;
