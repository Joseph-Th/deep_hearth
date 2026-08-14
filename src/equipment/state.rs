//! Persistent equipment records and cursor validation; sibling execution is the only mutation path.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::core::time::SimulationTick;
use crate::maintenance::Condition;
use crate::structural::StructuralElementId;

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
    pub(super) supported_by: Option<StructuralElementId>,
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

    /// Returns the structural member currently carrying this equipment's weight, if assigned.
    #[must_use]
    pub const fn supported_by(&self) -> Option<StructuralElementId> {
        self.supported_by
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
    pub(super) equipment_by_support: BTreeMap<StructuralElementId, BTreeSet<EquipmentId>>,
}

impl EquipmentState {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            revision: 0,
            next_equipment_id: 1,
            records: BTreeMap::new(),
            equipment_by_support: BTreeMap::new(),
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

    /// Iterates equipment assigned to one structural support in stable equipment-ID order.
    pub(crate) fn supported_equipment(
        &self,
        support: StructuralElementId,
    ) -> impl Iterator<Item = EquipmentId> + '_ {
        self.equipment_by_support
            .get(&support)
            .into_iter()
            .flat_map(|equipment| equipment.iter().copied())
    }

    pub(super) fn apply_support_change(
        &mut self,
        equipment: EquipmentId,
        before: Option<StructuralElementId>,
        after: Option<StructuralElementId>,
        next_revision: u64,
    ) {
        if let Some(before) = before {
            let remove_entry = {
                let indexed = match self.equipment_by_support.get_mut(&before) {
                    Some(indexed) => indexed,
                    None => panic!(
                        "runtime invariant broken: support index missing element {} for equipment {}",
                        before.value(),
                        equipment.value()
                    ),
                };
                assert!(
                    indexed.remove(&equipment),
                    "runtime invariant broken: support index element {} missing equipment {}",
                    before.value(),
                    equipment.value()
                );
                indexed.is_empty()
            };
            if remove_entry {
                self.equipment_by_support.remove(&before);
            }
        }
        if let Some(after) = after {
            let inserted = self
                .equipment_by_support
                .entry(after)
                .or_default()
                .insert(equipment);
            assert!(
                inserted,
                "runtime invariant broken: support index element {} already contains equipment {}",
                after.value(),
                equipment.value()
            );
        }
        let record = match self.records.get_mut(&equipment) {
            Some(record) => record,
            None => panic!(
                "runtime invariant broken: equipment {} disappeared during support update",
                equipment.value()
            ),
        };
        debug_assert_eq!(record.supported_by, before);
        record.supported_by = after;
        self.revision = next_revision;
    }

    pub(crate) fn has_valid_id_cursor(&self) -> bool {
        self.next_equipment_id != 0
            && self
                .records
                .keys()
                .next_back()
                .is_none_or(|id| id.value() < self.next_equipment_id)
    }

    pub(crate) fn has_valid_support_index(&self) -> bool {
        let records_match_index = self
            .records
            .values()
            .all(|record| match record.supported_by {
                Some(support) => self
                    .equipment_by_support
                    .get(&support)
                    .is_some_and(|equipment| equipment.contains(&record.id)),
                None => true,
            });
        let index_matches_records = self
            .equipment_by_support
            .iter()
            .all(|(support, equipment)| {
                support.value() != 0
                    && !equipment.is_empty()
                    && equipment.iter().all(|id| {
                        id.value() != 0
                            && self
                                .records
                                .get(id)
                                .is_some_and(|record| record.supported_by == Some(*support))
                    })
            });
        records_match_index && index_matches_records
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
    ZeroSupportElementId {
        equipment: EquipmentId,
    },
    ZeroIndexedSupportElementId,
    ZeroIndexedEquipmentId {
        element: StructuralElementId,
    },
    EmptySupportIndex {
        element: StructuralElementId,
    },
    MissingSupportIndex {
        equipment: EquipmentId,
        element: StructuralElementId,
    },
    UnknownIndexedEquipment {
        equipment: EquipmentId,
        element: StructuralElementId,
    },
    SupportIndexMismatch {
        equipment: EquipmentId,
        indexed: StructuralElementId,
        actual: Option<StructuralElementId>,
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
            Self::ZeroSupportElementId { equipment } => write!(
                formatter,
                "equipment {} references zero structural support id",
                equipment.value()
            ),
            Self::ZeroIndexedSupportElementId => {
                formatter.write_str("equipment support reverse index contains zero structural id")
            }
            Self::ZeroIndexedEquipmentId { element } => write!(
                formatter,
                "equipment support reverse index for element {} contains zero equipment id",
                element.value()
            ),
            Self::EmptySupportIndex { element } => write!(
                formatter,
                "equipment support reverse index contains empty entry for element {}",
                element.value()
            ),
            Self::MissingSupportIndex { equipment, element } => write!(
                formatter,
                "equipment {} references support element {} but is absent from the reverse index",
                equipment.value(),
                element.value()
            ),
            Self::UnknownIndexedEquipment { equipment, element } => write!(
                formatter,
                "equipment support reverse index element {} references missing equipment {}",
                element.value(),
                equipment.value()
            ),
            Self::SupportIndexMismatch {
                equipment,
                indexed,
                actual,
            } => write!(
                formatter,
                "equipment support reverse index places equipment {} on element {} but record support is {actual:?}",
                equipment.value(),
                indexed.value()
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
        if record
            .supported_by
            .is_some_and(|element| element.value() == 0)
        {
            return Err(EquipmentValidationError::ZeroSupportElementId {
                equipment: record.id,
            });
        }
        if let Some(element) = record.supported_by
            && !state
                .equipment_by_support
                .get(&element)
                .is_some_and(|equipment| equipment.contains(&record.id))
        {
            return Err(EquipmentValidationError::MissingSupportIndex {
                equipment: record.id,
                element,
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

    for (element, equipment_ids) in &state.equipment_by_support {
        if element.value() == 0 {
            return Err(EquipmentValidationError::ZeroIndexedSupportElementId);
        }
        if equipment_ids.is_empty() {
            return Err(EquipmentValidationError::EmptySupportIndex { element: *element });
        }
        for equipment in equipment_ids {
            if equipment.value() == 0 {
                return Err(EquipmentValidationError::ZeroIndexedEquipmentId { element: *element });
            }
            let Some(record) = state.records.get(equipment) else {
                return Err(EquipmentValidationError::UnknownIndexedEquipment {
                    equipment: *equipment,
                    element: *element,
                });
            };
            if record.supported_by != Some(*element) {
                return Err(EquipmentValidationError::SupportIndexMismatch {
                    equipment: *equipment,
                    indexed: *element,
                    actual: record.supported_by,
                });
            }
        }
    }

    Ok(())
}
