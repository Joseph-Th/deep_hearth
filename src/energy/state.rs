//! Persistent finite-energy ownership with immutable definition references.

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
    pub(crate) id: EnergyStoreId,
    pub(crate) definition: EnergyStoreDefinitionId,
    pub(crate) stored: Energy,
    pub(crate) created_at: SimulationTick,
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
    pub(crate) revision: u64,
    pub(crate) next_store_id: u64,
    pub(crate) records: BTreeMap<EnergyStoreId, EnergyStoreRecord>,
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
    pub fn get_store(&self, id: EnergyStoreId) -> Option<&EnergyStoreRecord> {
        self.records.get(&id)
    }

    pub fn stores(&self) -> impl Iterator<Item = &EnergyStoreRecord> {
        self.records.values()
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

/// Invalid persisted energy ownership discovered during exhaustive load validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnergyValidationError {
    InvalidIdCursor,
    RecordKeyMismatch {
        key: EnergyStoreId,
        record: EnergyStoreId,
    },
    UnknownDefinition {
        store: EnergyStoreId,
        definition: EnergyStoreDefinitionId,
    },
    CapacityExceeded {
        store: EnergyStoreId,
        stored: Energy,
        capacity: Energy,
    },
    CreatedInFuture {
        store: EnergyStoreId,
        created_at: SimulationTick,
        current: SimulationTick,
    },
}

impl Display for EnergyValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidIdCursor => formatter.write_str("energy store ID cursor is invalid"),
            Self::RecordKeyMismatch { key, record } => write!(
                formatter,
                "energy store map key {} disagrees with record id {}",
                key.value(),
                record.value()
            ),
            Self::UnknownDefinition { store, definition } => write!(
                formatter,
                "energy store {} references unknown definition {}",
                store.value(),
                definition.value()
            ),
            Self::CapacityExceeded {
                store,
                stored,
                capacity,
            } => write!(
                formatter,
                "energy store {} contains {} nJ above capacity {} nJ",
                store.value(),
                stored.nanojoules(),
                capacity.nanojoules()
            ),
            Self::CreatedInFuture {
                store,
                created_at,
                current,
            } => write!(
                formatter,
                "energy store {} was created at tick {} after current tick {}",
                store.value(),
                created_at.value(),
                current.value()
            ),
        }
    }
}

impl Error for EnergyValidationError {}

pub(crate) fn validate_loaded_energy(
    registry: &EnergyRegistry,
    state: &EnergyState,
    current: SimulationTick,
) -> Result<(), EnergyValidationError> {
    if !state.has_valid_id_cursor() {
        return Err(EnergyValidationError::InvalidIdCursor);
    }
    for (key, record) in &state.records {
        if *key != record.id {
            return Err(EnergyValidationError::RecordKeyMismatch {
                key: *key,
                record: record.id,
            });
        }
        let Some(definition) = registry.get_store(record.definition) else {
            return Err(EnergyValidationError::UnknownDefinition {
                store: record.id,
                definition: record.definition,
            });
        };
        if record.stored > definition.capacity() {
            return Err(EnergyValidationError::CapacityExceeded {
                store: record.id,
                stored: record.stored,
                capacity: definition.capacity(),
            });
        }
        if record.created_at > current {
            return Err(EnergyValidationError::CreatedInFuture {
                store: record.id,
                created_at: record.created_at,
                current,
            });
        }
    }
    Ok(())
}
