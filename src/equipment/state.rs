//! Persistent equipment records and cursor validation; sibling execution is the only mutation path.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::core::time::SimulationTick;
use crate::maintenance::Condition;

use super::definitions::{EquipmentDefinitionId, EquipmentRegistry};

/// Persistent identifier for one runtime equipment record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EquipmentId(u32);

impl EquipmentId {
    #[must_use]
    pub const fn new(value: u32) -> Self {
        assert!(value != 0, "equipment id must be nonzero");
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }
}

/// Persistent mutable state of one maintainable equipment instance.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EquipmentRecord {
    pub(super) id: EquipmentId,
    pub(super) definition: EquipmentDefinitionId,
    pub(super) condition: Condition,
    pub(super) created_at: SimulationTick,
}

/// Persistent provenance of the equipment instance that authorized an in-flight operation.
///
/// Production owns exclusivity while the job is active; this trace preserves the provider
/// definition and condition that were validated at operation resolution time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EquipmentOperationTrace {
    equipment: EquipmentId,
    definition: EquipmentDefinitionId,
    condition: Condition,
}

impl EquipmentOperationTrace {
    pub(crate) const fn new(
        equipment: EquipmentId,
        definition: EquipmentDefinitionId,
        condition: Condition,
    ) -> Self {
        Self {
            equipment,
            definition,
            condition,
        }
    }

    #[must_use]
    pub const fn equipment(self) -> EquipmentId {
        self.equipment
    }

    #[must_use]
    pub const fn definition(self) -> EquipmentDefinitionId {
        self.definition
    }

    #[must_use]
    pub const fn condition(self) -> Condition {
        self.condition
    }
}

impl EquipmentRecord {
    #[must_use]
    pub const fn id(&self) -> EquipmentId {
        self.id
    }

    #[must_use]
    pub const fn definition(&self) -> EquipmentDefinitionId {
        self.definition
    }

    #[must_use]
    pub const fn condition(&self) -> Condition {
        self.condition
    }

    #[must_use]
    pub const fn created_at(&self) -> SimulationTick {
        self.created_at
    }
}

/// Authoritative equipment collection and monotonic mutation/version state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EquipmentState {
    pub(super) revision: u64,
    pub(super) next_equipment_id: u32,
    pub(super) records: BTreeMap<EquipmentId, EquipmentRecord>,
}

impl EquipmentState {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            revision: 0,
            next_equipment_id: 1,
            records: BTreeMap::new(),
        }
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub fn get_equipment(&self, id: EquipmentId) -> Option<&EquipmentRecord> {
        self.records.get(&id)
    }

    pub fn equipment(&self) -> impl Iterator<Item = &EquipmentRecord> {
        self.records.values()
    }

    pub(crate) fn has_valid_id_cursor(&self) -> bool {
        self.next_equipment_id != 0
            && self
                .records
                .keys()
                .next_back()
                .is_none_or(|id| id.value() < self.next_equipment_id)
    }
}

/// Structural or cross-reference failure in decoded persistent equipment state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EquipmentValidationError {
    ZeroNextEquipmentId,
    ZeroEquipmentId,
    KeyIdMismatch {
        key: EquipmentId,
        record: EquipmentId,
    },
    NextEquipmentIdNotAboveAllocated {
        next: u32,
        highest: EquipmentId,
    },
    ZeroDefinitionId {
        equipment: EquipmentId,
    },
    UnknownDefinition {
        equipment: EquipmentId,
        definition: EquipmentDefinitionId,
    },
    CreatedInFuture {
        equipment: EquipmentId,
        created_at: SimulationTick,
        current: SimulationTick,
    },
}

impl Display for EquipmentValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroNextEquipmentId => {
                formatter.write_str("equipment next-id cursor must be nonzero")
            }
            Self::ZeroEquipmentId => formatter.write_str("equipment record id must be nonzero"),
            Self::KeyIdMismatch { key, record } => write!(
                formatter,
                "equipment map key {} disagrees with record id {}",
                key.value(),
                record.value()
            ),
            Self::NextEquipmentIdNotAboveAllocated { next, highest } => write!(
                formatter,
                "equipment next-id cursor {next} is not above allocated id {}",
                highest.value()
            ),
            Self::ZeroDefinitionId { equipment } => write!(
                formatter,
                "equipment {} has zero definition id",
                equipment.value()
            ),
            Self::UnknownDefinition {
                equipment,
                definition,
            } => write!(
                formatter,
                "equipment {} references unknown definition {}",
                equipment.value(),
                definition.value()
            ),
            Self::CreatedInFuture {
                equipment,
                created_at,
                current,
            } => write!(
                formatter,
                "equipment {} was created at tick {} after current tick {}",
                equipment.value(),
                created_at.value(),
                current.value()
            ),
        }
    }
}

impl Error for EquipmentValidationError {}

pub(crate) fn validate_loaded_equipment(
    definitions: &EquipmentRegistry,
    state: &EquipmentState,
    current_tick: SimulationTick,
) -> Result<(), EquipmentValidationError> {
    if state.next_equipment_id == 0 {
        return Err(EquipmentValidationError::ZeroNextEquipmentId);
    }

    if let Some(highest) = state.records.keys().next_back().copied()
        && highest.value() >= state.next_equipment_id
    {
        return Err(EquipmentValidationError::NextEquipmentIdNotAboveAllocated {
            next: state.next_equipment_id,
            highest,
        });
    }

    for (key, record) in &state.records {
        if key.value() == 0 || record.id.value() == 0 {
            return Err(EquipmentValidationError::ZeroEquipmentId);
        }
        if *key != record.id {
            return Err(EquipmentValidationError::KeyIdMismatch {
                key: *key,
                record: record.id,
            });
        }
        if record.definition.value() == 0 {
            return Err(EquipmentValidationError::ZeroDefinitionId {
                equipment: record.id,
            });
        }
        if definitions.get_equipment(record.definition).is_none() {
            return Err(EquipmentValidationError::UnknownDefinition {
                equipment: record.id,
                definition: record.definition,
            });
        }
        if record.created_at > current_tick {
            return Err(EquipmentValidationError::CreatedInFuture {
                equipment: record.id,
                created_at: record.created_at,
                current: current_tick,
            });
        }
    }

    Ok(())
}
