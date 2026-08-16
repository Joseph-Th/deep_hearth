//! Persistent-state validation for equipment; this child audits private owner data without exposing mutation.

use super::*;

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
