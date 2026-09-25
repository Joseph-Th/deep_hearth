//! Validates persisted equipment records, embodiment, support indexes, and authored references.

use crate::core::time::SimulationTick;
use crate::material::MaterialRegistry;
use crate::structural::{SupportIndexValidationFault, validate_support_index};

use super::super::definitions::{EquipmentDefinition, EquipmentRegistry};
use super::{EquipmentId, EquipmentRecord, EquipmentState};

mod embodiment;
mod error;

use embodiment::validate_equipment_material;
pub use error::EquipmentValidationError;

pub(crate) fn validate_loaded_equipment(
    definitions: &EquipmentRegistry,
    materials: &MaterialRegistry,
    state: &EquipmentState,
    current_tick: SimulationTick,
) -> Result<(), EquipmentValidationError> {
    if state.support_revision > state.revision {
        return Err(EquipmentValidationError::SupportRevisionAfterRevision {
            support_revision: state.support_revision,
            revision: state.revision,
        });
    }
    validate_equipment_cursor(state)?;
    for (key, record) in &state.records {
        validate_equipment_record(definitions, materials, state, *key, record, current_tick)?;
    }
    validate_equipment_support_index(state)
}

fn validate_equipment_cursor(state: &EquipmentState) -> Result<(), EquipmentValidationError> {
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
    Ok(())
}

fn validate_equipment_record(
    definitions: &EquipmentRegistry,
    materials: &MaterialRegistry,
    state: &EquipmentState,
    key: EquipmentId,
    record: &EquipmentRecord,
    current_tick: SimulationTick,
) -> Result<(), EquipmentValidationError> {
    if key.value() == 0 || record.id.value() == 0 {
        return Err(EquipmentValidationError::ZeroEquipmentId);
    }
    if key != record.id {
        return Err(EquipmentValidationError::KeyIdMismatch {
            key,
            record: record.id,
        });
    }
    if record.definition.value() == 0 {
        return Err(EquipmentValidationError::ZeroDefinitionId {
            equipment: record.id,
        });
    }
    validate_equipment_support_reference(state, record)?;
    let Some(definition) = definitions.get_equipment(record.definition) else {
        return Err(EquipmentValidationError::UnknownDefinition {
            equipment: record.id,
            definition: record.definition,
        });
    };
    validate_maintenance_admission(state, record, definition, current_tick)?;
    validate_equipment_material(definitions, materials, record, definition, current_tick)?;
    if record.created_at > current_tick {
        return Err(EquipmentValidationError::CreatedInFuture {
            equipment: record.id,
            created_at: record.created_at,
            current: current_tick,
        });
    }
    Ok(())
}

fn validate_maintenance_admission(
    state: &EquipmentState,
    record: &EquipmentRecord,
    definition: &EquipmentDefinition,
    current_tick: SimulationTick,
) -> Result<(), EquipmentValidationError> {
    let Some(admission) = record.last_maintenance_admission else {
        return Ok(());
    };
    if admission.equipment_revision() == 0 || admission.equipment_revision() > state.revision {
        return Err(
            EquipmentValidationError::MaintenanceAdmissionRevisionInvalid {
                equipment: record.id,
                admission_revision: admission.equipment_revision(),
                current_revision: state.revision,
            },
        );
    }
    if admission.admitted_at() < record.created_at || admission.admitted_at() > current_tick {
        return Err(EquipmentValidationError::MaintenanceAdmissionTickInvalid {
            equipment: record.id,
            admitted_at: admission.admitted_at(),
            created_at: record.created_at,
            current: current_tick,
        });
    }
    let Some(profile) = definition.maintenance_profile() else {
        return Err(
            EquipmentValidationError::MaintenanceAdmissionProfileMissing {
                equipment: record.id,
            },
        );
    };
    if admission.condition_before() >= admission.condition_after()
        || admission.condition_after() != profile.restored_condition()
    {
        return Err(
            EquipmentValidationError::MaintenanceAdmissionOutcomeInvalid {
                equipment: record.id,
                before: admission.condition_before(),
                after: admission.condition_after(),
                required: profile.restored_condition(),
            },
        );
    }
    Ok(())
}

fn validate_equipment_support_reference(
    state: &EquipmentState,
    record: &EquipmentRecord,
) -> Result<(), EquipmentValidationError> {
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
    Ok(())
}

fn validate_equipment_support_index(
    state: &EquipmentState,
) -> Result<(), EquipmentValidationError> {
    validate_support_index(
        &state.equipment_by_support,
        |equipment| equipment.value() == 0,
        |equipment| {
            state
                .records
                .get(&equipment)
                .map(|record| record.supported_by)
        },
    )
    .map_err(|fault| match fault {
        SupportIndexValidationFault::ZeroSupportElementId => {
            EquipmentValidationError::ZeroIndexedSupportElementId
        }
        SupportIndexValidationFault::EmptySupportBucket { element } => {
            EquipmentValidationError::EmptySupportIndex { element }
        }
        SupportIndexValidationFault::InvalidItemId { element, .. } => {
            EquipmentValidationError::ZeroIndexedEquipmentId { element }
        }
        SupportIndexValidationFault::UnknownIndexedItem { item, element } => {
            EquipmentValidationError::UnknownIndexedEquipment {
                equipment: item,
                element,
            }
        }
        SupportIndexValidationFault::SupportMismatch {
            item,
            indexed,
            actual,
        } => EquipmentValidationError::SupportIndexMismatch {
            equipment: item,
            indexed,
            actual,
        },
    })
}
