//! Trusted-load replay validation for direct equipment-maintenance labor.

use crate::core::quantity::{Energy, Volume};
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::equipment::{EquipmentOccupancy, equipment_occupancy};
use crate::labor::EquipmentMaintenanceWork;
use crate::registry::Registries;

use super::{ActivePlayerJobs, PlayerWorkValidationError, validate_remaining_resources};

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
    if work.condition_after() != profile.restored_condition()
        || work.condition_after() <= work.condition_before()
    {
        return Err(PlayerWorkValidationError::EquipmentMaintenanceTargetMismatch);
    }
    if work.started_at() > state.tick()
        || work.completes_at() <= state.tick()
        || work.completes_at() <= work.started_at()
    {
        return Err(PlayerWorkValidationError::EquipmentMaintenanceScheduleInvalid);
    }
    let required_duration = profile.required_service_duration(work.condition_before());
    let actual_duration = work.completes_at().value() - work.started_at().value();
    if actual_duration != required_duration.value() {
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
        available_energy,
        available_hydration,
        profile.exertion(),
        TickSpan::new(work.completes_at().value() - state.tick().value()),
    )
}
