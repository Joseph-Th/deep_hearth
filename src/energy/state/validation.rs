//! Persistent-state validation for energy; this child audits private owner data without exposing mutation.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Energy;
use crate::core::time::SimulationTick;

use super::super::definitions::{EnergyRegistry, EnergyStoreDefinitionId};
use super::{EnergyState, EnergyStoreId};

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
