//! Authorization-stage validation for one already-resolved equipment maintenance operation.

use crate::core::state::AppState;
use crate::registry::Registries;

use super::material::validate_maintenance_material;
use super::{EquipmentMaintenanceError, ValidatedEquipmentMaintenance};
use crate::equipment::maintenance_resolution::{
    EquipmentMaintenanceResolution, impure_replacement_commodity,
};

/// Validates one already-resolved, resource-backed equipment maintenance without mutating any owner.
pub fn validate_equipment_maintenance(
    registries: &Registries,
    state: &AppState,
    resolution: EquipmentMaintenanceResolution,
) -> Result<ValidatedEquipmentMaintenance, EquipmentMaintenanceError> {
    let equipment = resolution.equipment;
    let record = state
        .equipment()
        .get_equipment(equipment)
        .ok_or(EquipmentMaintenanceError::UnknownEquipment { equipment })?;
    let actual_equipment_revision = state.equipment().revision();
    if actual_equipment_revision != resolution.expected_equipment_revision {
        return Err(EquipmentMaintenanceError::StaleEquipmentResolution {
            equipment,
            expected_revision: resolution.expected_equipment_revision,
            actual_revision: actual_equipment_revision,
        });
    }
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
    if let Some(job) = state.production().get_equipment_occupant(equipment) {
        return Err(EquipmentMaintenanceError::EquipmentBusy {
            equipment,
            job: job.id(),
            release: job.occupancy_release(),
        });
    }
    let condition_before = resolution.condition_before;
    let condition_after = resolution.condition_after;
    if condition_after <= condition_before {
        return Err(EquipmentMaintenanceError::ConditionNotImproved {
            equipment,
            before: condition_before,
            after: condition_after,
        });
    }
    if let Some(commodity) = impure_replacement_commodity(&resolution.material) {
        return Err(EquipmentMaintenanceError::ImpureReplacementMaterial { commodity });
    }
    let next_equipment_revision = state
        .equipment()
        .revision()
        .checked_add(1)
        .ok_or(EquipmentMaintenanceError::EquipmentRevisionExhausted)?;
    let expected_equipment_revision = state.equipment().revision();
    let material = validate_maintenance_material(registries, state, record, resolution)
        .map_err(EquipmentMaintenanceError::Material)?;

    Ok(ValidatedEquipmentMaintenance {
        equipment,
        condition_before,
        condition_after,
        expected_equipment_revision,
        next_equipment_revision,
        material,
    })
}
