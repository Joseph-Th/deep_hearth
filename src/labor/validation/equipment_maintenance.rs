//! Trusted-load replay validation for direct equipment-maintenance labor.

use crate::core::quantity::{Energy, Volume};
use crate::core::state::AppState;
use crate::equipment::{EquipmentOccupancy, equipment_occupancy};
use crate::labor::EquipmentMaintenanceWork;
use crate::registry::Registries;

use super::{
    ActivePlayerJobs, PlayerWorkValidationError, project_active_work_schedule,
    validate_remaining_resources,
};

pub(super) fn validate_equipment_maintenance_work(
    registries: &Registries,
    state: &AppState,
    active_jobs: &ActivePlayerJobs,
    work: EquipmentMaintenanceWork,
    available_energy: Energy,
    available_hydration: Volume,
) -> Result<(), PlayerWorkValidationError> {
    if active_jobs.has_any() {
        return Err(PlayerWorkValidationError::EquipmentMaintenanceResourceDoubleBooked);
    }
    if state.equipment().revision().checked_add(1).is_none() {
        return Err(PlayerWorkValidationError::EquipmentMaintenanceEquipmentRevisionExhausted);
    }
    let record = state
        .equipment()
        .get_equipment(work.equipment())
        .ok_or(PlayerWorkValidationError::EquipmentMaintenanceEquipmentMissing)?;
    if record.definition() != work.equipment_trace().definition() {
        return Err(PlayerWorkValidationError::EquipmentMaintenanceDefinitionMismatch);
    }
    if record.condition() != work.condition_before() {
        return Err(PlayerWorkValidationError::EquipmentMaintenanceConditionMismatch);
    }
    let profile = registries
        .equipment()
        .get_equipment(record.definition())
        .and_then(|definition| definition.maintenance_profile())
        .ok_or(PlayerWorkValidationError::EquipmentMaintenanceProfileMissing)?;
    let admission = record
        .last_maintenance_admission()
        .ok_or(PlayerWorkValidationError::EquipmentMaintenanceAdmissionMissing)?;
    if admission.equipment_revision() != work.admission_revision()
        || admission.condition_before() != work.condition_before()
        || admission.condition_after() != work.condition_after()
        || admission.admitted_at() != work.started_at()
    {
        return Err(PlayerWorkValidationError::EquipmentMaintenanceAdmissionMismatch);
    }
    if work.condition_after() != profile.restored_condition()
        || work.condition_after() <= work.condition_before()
    {
        return Err(PlayerWorkValidationError::EquipmentMaintenanceTargetMismatch);
    }
    let schedule =
        project_active_work_schedule(state.tick(), work.started_at(), work.completes_at())
            .ok_or(PlayerWorkValidationError::EquipmentMaintenanceScheduleInvalid)?;
    let required_duration = profile.required_service_duration(work.condition_before());
    if schedule.duration != required_duration {
        return Err(PlayerWorkValidationError::EquipmentMaintenanceDurationMismatch);
    }
    if matches!(
        equipment_occupancy(state, work.equipment()),
        Some(EquipmentOccupancy::Production { .. } | EquipmentOccupancy::Mining { .. })
    ) {
        return Err(PlayerWorkValidationError::EquipmentMaintenanceResourceDoubleBooked);
    }
    validate_remaining_resources(
        registries,
        state,
        available_energy,
        available_hydration,
        profile.exertion(),
        schedule.remaining,
    )
}
