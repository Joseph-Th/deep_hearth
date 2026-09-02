//! Authorization-stage validation for one already-resolved equipment maintenance operation.

use crate::core::state::AppState;
use crate::equipment::{EquipmentOperationTrace, EquipmentRecord};
use crate::labor::{EquipmentMaintenanceWork, PlayerWork, validate_player_work_start};
use crate::registry::Registries;

use super::material::validate_maintenance_material;
use super::{EquipmentMaintenanceError, ValidatedEquipmentMaintenance};
use crate::equipment::maintenance_resolution::{
    EquipmentMaintenanceResolution, impure_replacement_commodity,
};

fn validate_equipment_availability(
    state: &AppState,
    equipment: crate::equipment::EquipmentId,
) -> Result<(), EquipmentMaintenanceError> {
    if let Some(job) = state.mining().get_equipment_occupant(equipment) {
        return Err(EquipmentMaintenanceError::EquipmentBusyMining { equipment, job });
    }
    if state
        .player_work()
        .get_manual_power_equipment_occupant(equipment)
        .is_some()
    {
        return Err(EquipmentMaintenanceError::EquipmentBusyManualPower { equipment });
    }
    if let Some(job) = state.production().get_equipment_occupant(equipment) {
        return Err(EquipmentMaintenanceError::EquipmentBusy {
            equipment,
            job: job.id(),
            release: job.occupancy_release(),
        });
    }
    Ok(())
}

fn validate_resolved_equipment<'a>(
    registries: &Registries,
    state: &'a AppState,
    resolution: &EquipmentMaintenanceResolution,
) -> Result<&'a EquipmentRecord, EquipmentMaintenanceError> {
    let equipment = resolution.equipment;
    let record = state
        .equipment()
        .get_equipment(equipment)
        .ok_or(EquipmentMaintenanceError::UnknownEquipment { equipment })?;
    let actual_revision = state.equipment().revision();
    if actual_revision != resolution.expected_equipment_revision {
        return Err(EquipmentMaintenanceError::StaleEquipmentResolution {
            equipment,
            expected_revision: resolution.expected_equipment_revision,
            actual_revision,
        });
    }
    if record.condition() != resolution.condition_before {
        return Err(EquipmentMaintenanceError::ConditionChangedSinceResolution {
            equipment,
            expected: resolution.condition_before,
            actual: record.condition(),
        });
    }
    if registries
        .equipment()
        .get_equipment(record.definition())
        .is_none()
    {
        return Err(EquipmentMaintenanceError::UnknownDefinition {
            equipment,
            definition: record.definition(),
        });
    }
    validate_equipment_availability(state, equipment)?;
    Ok(record)
}

fn validate_resolved_outcome(
    resolution: &EquipmentMaintenanceResolution,
) -> Result<(), EquipmentMaintenanceError> {
    if resolution.condition_after <= resolution.condition_before {
        return Err(EquipmentMaintenanceError::ConditionNotImproved {
            equipment: resolution.equipment,
            before: resolution.condition_before,
            after: resolution.condition_after,
        });
    }
    if let Some(commodity) = impure_replacement_commodity(&resolution.material) {
        return Err(EquipmentMaintenanceError::ImpureReplacementMaterial { commodity });
    }
    Ok(())
}

/// Validates one already-resolved, resource-backed equipment maintenance without mutating any owner.
pub fn validate_equipment_maintenance(
    registries: &Registries,
    state: &AppState,
    resolution: EquipmentMaintenanceResolution,
) -> Result<ValidatedEquipmentMaintenance, EquipmentMaintenanceError> {
    let equipment = resolution.equipment;
    let record = validate_resolved_equipment(registries, state, &resolution)?;
    let condition_before = resolution.condition_before;
    let condition_after = resolution.condition_after;
    validate_resolved_outcome(&resolution)?;
    let next_equipment_revision = state
        .equipment()
        .revision()
        .checked_add(1)
        .ok_or(EquipmentMaintenanceError::EquipmentRevisionExhausted)?;
    let expected_equipment_revision = state.equipment().revision();
    let duration = resolution.duration;
    let exertion = resolution.exertion;
    let material = validate_maintenance_material(registries, state, record, resolution)
        .map_err(EquipmentMaintenanceError::Material)?;
    let completes_at = state.tick().checked_add_span(duration).ok_or(
        EquipmentMaintenanceError::CompletionTickOverflow {
            current: state.tick(),
            duration,
        },
    )?;
    let work = EquipmentMaintenanceWork::new(
        EquipmentOperationTrace::new(equipment, record.definition(), condition_before),
        condition_after,
        state.tick(),
        completes_at,
    );
    let player_work = validate_player_work_start(
        registries,
        state,
        PlayerWork::EquipmentMaintenance { work },
        duration,
        exertion,
    )
    .map_err(EquipmentMaintenanceError::PlayerWork)?;

    Ok(ValidatedEquipmentMaintenance {
        equipment,
        condition_before,
        condition_after,
        expected_equipment_revision,
        next_equipment_revision,
        material,
        work,
        player_work,
    })
}
