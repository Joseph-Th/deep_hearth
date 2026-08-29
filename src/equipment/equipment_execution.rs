//! Fixture-only allocation for authored equipment that has no ordinary assembly path.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::state::AppState;
use crate::maintenance::Condition;
#[cfg(any(test, feature = "test-gameplay"))]
use crate::registry::Registries;

#[cfg(any(test, feature = "test-gameplay"))]
use super::definitions::EquipmentDefinitionId;
use super::state::EquipmentId;
#[cfg(any(test, feature = "test-gameplay"))]
use super::state::EquipmentRecord;

/// Failure while allocating one persistent equipment instance.
#[cfg(any(test, feature = "test-gameplay"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AddEquipmentError {
    UnknownDefinition { definition: EquipmentDefinitionId },
    RequiresAssembly { definition: EquipmentDefinitionId },
    IdExhausted,
    RevisionExhausted,
}

#[cfg(any(test, feature = "test-gameplay"))]
impl Display for AddEquipmentError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownDefinition { definition } => write!(
                formatter,
                "unknown equipment definition {}",
                definition.value()
            ),
            Self::RequiresAssembly { definition } => write!(
                formatter,
                "equipment definition {} requires conserved gameplay assembly",
                definition.value()
            ),
            Self::IdExhausted => formatter.write_str("equipment identifier space is exhausted"),
            Self::RevisionExhausted => formatter.write_str("equipment revision space is exhausted"),
        }
    }
}

#[cfg(any(test, feature = "test-gameplay"))]
impl Error for AddEquipmentError {}

/// Adds one equipment record for tests and gameplay harness bootstrap fixtures.
#[cfg(any(test, feature = "test-gameplay"))]
pub(crate) fn add_equipment(
    registries: &Registries,
    state: &mut AppState,
    definition: EquipmentDefinitionId,
    condition: Condition,
) -> Result<EquipmentId, AddEquipmentError> {
    let Some(definition_record) = registries.equipment().get_equipment(definition) else {
        return Err(AddEquipmentError::UnknownDefinition { definition });
    };
    if definition_record.assembly_profile().is_some() {
        return Err(AddEquipmentError::RequiresAssembly { definition });
    }

    let equipment_state = state.equipment();
    let id = EquipmentId::new(equipment_state.next_equipment_id());
    let next_equipment_id = equipment_state
        .next_equipment_id()
        .checked_add(1)
        .ok_or(AddEquipmentError::IdExhausted)?;
    let next_revision = equipment_state
        .revision()
        .checked_add(1)
        .ok_or(AddEquipmentError::RevisionExhausted)?;
    let record = EquipmentRecord {
        id,
        definition,
        condition,
        embodied_mass: definition_record.mass(),
        embodied_material: Vec::new(),
        supported_by: None,
        created_at: state.tick(),
    };

    let equipment_state = state.equipment_state_mut();
    equipment_state.insert_equipment(record, next_equipment_id, next_revision);
    Ok(id)
}

#[cfg(test)]
#[path = "equipment_execution_tests.rs"]
mod tests;
