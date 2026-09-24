//! Equipment binding, support, and cross-owner occupancy admission.

use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::equipment::{
    EquipmentId, EquipmentOccupancy, EquipmentOperationTrace, ValidatedEquipmentUse,
    equipment_occupancy,
};
use crate::production::ProcessResolution;
use crate::structural::{StructuralElementId, StructuralLifecycle};

use super::super::StartProcessError;

#[must_use]
pub(in super::super) struct ValidatedEquipmentResources {
    pub(in super::super) selection: Option<ValidatedEquipmentUse>,
    pub(in super::super) provider: Option<EquipmentOperationTrace>,
}

pub(in super::super) fn validate_equipment_resources(
    state: &AppState,
    resolution: &ProcessResolution,
    completes_at: SimulationTick,
) -> Result<ValidatedEquipmentResources, StartProcessError> {
    let Some(selection) = resolution.equipment_use() else {
        return Ok(ValidatedEquipmentResources {
            selection: None,
            provider: None,
        });
    };
    let expected = selection.expected_equipment_revision();
    let actual = state.equipment().revision();
    if actual != expected {
        return Err(StartProcessError::StaleResolvedEquipment {
            expected_equipment_revision: expected,
            actual_equipment_revision: actual,
        });
    }
    let trace = selection.trace();
    let Some(record) = state.equipment().get_equipment(trace.equipment()) else {
        return Err(StartProcessError::ResolvedEquipmentMissing {
            equipment: trace.equipment(),
        });
    };
    if record.definition() != trace.definition() {
        return Err(StartProcessError::ResolvedEquipmentDefinitionChanged {
            equipment: trace.equipment(),
        });
    }
    if record.condition() != trace.condition() {
        return Err(StartProcessError::ResolvedEquipmentConditionChanged {
            equipment: trace.equipment(),
        });
    }
    let changes_condition = resolution
        .equipment_condition_after()
        .is_some_and(|after| after != trace.condition());
    let post_nonproduction_revision = state
        .checked_future_nonproduction_equipment_revision_demand()
        .and_then(|future| actual.checked_add(future))
        .ok_or(StartProcessError::EquipmentRevisionExhausted)?;
    if changes_condition
        && !state
            .production()
            .has_scheduled_equipment_revision_capacity_with_tick_from(
                post_nonproduction_revision,
                completes_at,
            )
    {
        return Err(StartProcessError::EquipmentRevisionExhausted);
    }
    if !changes_condition
        && !state
            .production()
            .has_scheduled_equipment_revision_capacity_from(post_nonproduction_revision)
    {
        return Err(StartProcessError::EquipmentRevisionExhausted);
    }
    validate_equipment_support(state, selection, record.supported_by())?;
    validate_equipment_available(state, trace.equipment())?;
    Ok(ValidatedEquipmentResources {
        selection: Some(selection),
        provider: Some(trace),
    })
}

fn validate_equipment_support(
    state: &AppState,
    selection: ValidatedEquipmentUse,
    actual_support: Option<StructuralElementId>,
) -> Result<(), StartProcessError> {
    let equipment = selection.trace().equipment();
    let expected_support = selection.support();
    if actual_support != expected_support {
        return Err(StartProcessError::ResolvedEquipmentSupportChanged {
            equipment,
            expected: expected_support,
            actual: actual_support,
        });
    }
    let Some(expected_structure_revision) = selection.expected_structure_revision() else {
        return Ok(());
    };
    let actual_structure_revision = state.structures().revision();
    if actual_structure_revision != expected_structure_revision {
        return Err(StartProcessError::StaleResolvedStructure {
            expected_structure_revision,
            actual_structure_revision,
        });
    }
    let element = expected_support.unwrap_or_else(|| {
        panic!("validated equipment use has structural revision without a support element")
    });
    let Some(support) = state.structures().get_element(element) else {
        return Err(StartProcessError::ResolvedEquipmentSupportMissing { equipment, element });
    };
    if support.lifecycle() != StructuralLifecycle::Active {
        return Err(StartProcessError::ResolvedEquipmentSupportNotActive {
            equipment,
            element,
            lifecycle: support.lifecycle(),
        });
    }
    Ok(())
}

fn validate_equipment_available(
    state: &AppState,
    equipment: EquipmentId,
) -> Result<(), StartProcessError> {
    match equipment_occupancy(state, equipment) {
        Some(EquipmentOccupancy::Production { job, release }) => {
            return Err(StartProcessError::EquipmentBusy {
                equipment,
                job,
                release,
            });
        }
        Some(EquipmentOccupancy::Mining { job }) => {
            return Err(StartProcessError::EquipmentBusyMining { equipment, job });
        }
        Some(EquipmentOccupancy::ManualPower { .. }) => {
            return Err(StartProcessError::EquipmentBusyManualPower { equipment });
        }
        Some(EquipmentOccupancy::Prospecting { completes_at }) => {
            return Err(StartProcessError::EquipmentBusyProspecting {
                equipment,
                completes_at,
            });
        }
        Some(EquipmentOccupancy::Maintenance { .. }) => {
            unreachable!("revision-current resolved equipment cannot already be under maintenance")
        }
        None => {}
    }
    Ok(())
}
