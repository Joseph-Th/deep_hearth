//! Controlled equipment fixture allocation and condition setup.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::state::AppState;
#[cfg(test)]
use crate::core::time::TickSpan;
use crate::maintenance::Condition;
#[cfg(test)]
use crate::maintenance::calculate_condition_after_active_ticks;
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
        last_maintenance_admission: None,
    };

    let equipment_state = state.equipment_state_mut();
    equipment_state.insert_equipment(record, next_equipment_id, next_revision);
    Ok(id)
}

/// Degrades idle equipment through the authoritative condition owner for controlled test setup.
///
/// This is deliberately not a gameplay command: it exposes no validation token or recoverable
/// legality model, and may only establish damaged fixture state while the equipment is unoccupied.
#[cfg(test)]
pub(crate) fn degrade_equipment_condition_for_test(
    state: &mut AppState,
    equipment: EquipmentId,
    wear_ppm: u32,
) {
    assert!(
        super::equipment_occupancy(state, equipment).is_none(),
        "equipment condition fixture cannot mutate occupied equipment {}",
        equipment.value()
    );
    let record = state
        .equipment()
        .get_equipment(equipment)
        .unwrap_or_else(|| {
            panic!(
                "equipment condition fixture references unknown equipment {}",
                equipment.value()
            )
        });
    let before = record.condition();
    let after = calculate_condition_after_active_ticks(wear_ppm, before, TickSpan::new(1));
    assert!(
        after < before,
        "equipment condition fixture must strictly degrade condition"
    );
    let next_revision = state
        .equipment()
        .revision()
        .checked_add(1)
        .unwrap_or_else(|| panic!("equipment condition fixture exhausted owner revision space"));
    state
        .equipment_state_mut()
        .apply_condition_change(equipment, before, after, next_revision);
}

#[cfg(test)]
#[path = "fixture_execution_tests.rs"]
mod tests;
