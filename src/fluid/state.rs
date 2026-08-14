//! Persistent finite fluid-store records; sibling execution owns all consequential mutation.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::core::quantity::{Temperature, Volume};
use crate::core::time::SimulationTick;

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

    #[must_use]
    pub const fn created_at(&self) -> SimulationTick {
        self.created_at
    }
}

/// Persistent owner for finite fluid stores and monotonic identity/revision cursors.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FluidState {
    pub(super) revision: u64,
    pub(super) next_store_id: u64,
    pub(super) records: BTreeMap<FluidStoreId, FluidStoreRecord>,
}

impl FluidState {
    #[must_use]
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
    pub fn get_store(&self, id: FluidStoreId) -> Option<&FluidStoreRecord> {
        self.records.get(&id)
    }

    pub fn stores(&self) -> impl Iterator<Item = &FluidStoreRecord> {
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

    pub(crate) fn has_valid_records(&self) -> bool {
        self.records.values().all(|record| {
            !record.capacity.is_zero()
                && record.contents.is_none_or(|contents| {
                    !contents.volume.is_zero() && contents.volume <= record.capacity
                })
        })
    }
}

/// Invalid persisted fluid ownership discovered during exhaustive load validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FluidValidationError {
    InvalidIdCursor,
    RecordKeyMismatch {
        key: FluidStoreId,
        record: FluidStoreId,
    },
    ZeroCapacity {
        store: FluidStoreId,
    },
    ZeroStoredVolume {
        store: FluidStoreId,
    },
    CapacityExceeded {
        store: FluidStoreId,
        stored: Volume,
        capacity: Volume,
    },
    UnknownDefinition {
        store: FluidStoreId,
        definition: FluidDefinitionId,
    },
    CreatedInFuture {
        store: FluidStoreId,
        created_at: SimulationTick,
        current: SimulationTick,
    },
}

impl Display for FluidValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidIdCursor => formatter.write_str("fluid store ID cursor is invalid"),
            Self::RecordKeyMismatch { key, record } => write!(
                formatter,
                "fluid store map key {} disagrees with record id {}",
                key.value(),
                record.value()
            ),
            Self::ZeroCapacity { store } => {
                write!(formatter, "fluid store {} has zero capacity", store.value())
            }
            Self::ZeroStoredVolume { store } => write!(
                formatter,
                "fluid store {} retains a fluid identity at zero volume",
                store.value()
            ),
            Self::CapacityExceeded {
                store,
                stored,
                capacity,
            } => write!(
                formatter,
                "fluid store {} contains {} uL above capacity {} uL",
                store.value(),
                stored.microliters(),
                capacity.microliters()
            ),
            Self::UnknownDefinition { store, definition } => write!(
                formatter,
                "fluid store {} references unknown fluid definition {}",
                store.value(),
                definition.value()
            ),
            Self::CreatedInFuture {
                store,
                created_at,
                current,
            } => write!(
                formatter,
                "fluid store {} was created at tick {} after current tick {}",
                store.value(),
                created_at.value(),
                current.value()
            ),
        }
    }
}

impl Error for FluidValidationError {}

pub(crate) fn validate_loaded_fluid(
    registry: &FluidRegistry,
    state: &FluidState,
    current: SimulationTick,
) -> Result<(), FluidValidationError> {
    if !state.has_valid_id_cursor() {
        return Err(FluidValidationError::InvalidIdCursor);
    }
    for (key, record) in &state.records {
        if *key != record.id {
            return Err(FluidValidationError::RecordKeyMismatch {
                key: *key,
                record: record.id,
            });
        }
        if record.capacity.is_zero() {
            return Err(FluidValidationError::ZeroCapacity { store: record.id });
        }
        if let Some(contents) = record.contents {
            if contents.volume.is_zero() {
                return Err(FluidValidationError::ZeroStoredVolume { store: record.id });
            }
            if contents.volume > record.capacity {
                return Err(FluidValidationError::CapacityExceeded {
                    store: record.id,
                    stored: contents.volume,
                    capacity: record.capacity,
                });
            }
            if registry.get_fluid(contents.fluid).is_none() {
                return Err(FluidValidationError::UnknownDefinition {
                    store: record.id,
                    definition: contents.fluid,
                });
            }
        }
        if record.created_at > current {
            return Err(FluidValidationError::CreatedInFuture {
                store: record.id,
                created_at: record.created_at,
                current,
            });
        }
    }
    Ok(())
}
