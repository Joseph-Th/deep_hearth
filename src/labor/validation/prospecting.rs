//! Trusted-load replay for active prospecting work.

use crate::core::quantity::{Energy, Volume};
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::equipment::{EquipmentOccupancy, EquipmentOperationTrace, equipment_occupancy};
use crate::maintenance::{Condition, calculate_usable_condition_after_active_ticks};
use crate::registry::Registries;

use super::{ActivePlayerJobs, PlayerWorkValidationError, validate_remaining_resources};
use crate::labor::{ProspectingDefinition, ProspectingEquipmentProfile, ProspectingWork};

fn validate_equipment_trace(
    state: &AppState,
    profile: ProspectingEquipmentProfile,
    trace: EquipmentOperationTrace,
    condition_after: Condition,
    duration: TickSpan,
) -> Result<(), PlayerWorkValidationError> {
    let equipment = trace.equipment();
    let record = state
        .equipment()
        .get_equipment(equipment)
        .ok_or(PlayerWorkValidationError::ProspectingEquipmentMissing)?;
    if record.definition() != trace.definition() {
        return Err(PlayerWorkValidationError::ProspectingEquipmentDefinitionMismatch);
    }
    if !profile.accepts(record.definition()) {
        return Err(
            PlayerWorkValidationError::ProspectingEquipmentDefinitionNotAccepted { equipment },
        );
    }
    if record.condition() != trace.condition() {
        return Err(PlayerWorkValidationError::ProspectingEquipmentConditionMismatch);
    }
    if record.supported_by().is_some() {
        return Err(PlayerWorkValidationError::ProspectingEquipmentMounted { equipment });
    }
    if matches!(
        equipment_occupancy(state, equipment),
        Some(EquipmentOccupancy::Production { .. } | EquipmentOccupancy::Mining { .. })
    ) {
        return Err(
            PlayerWorkValidationError::ProspectingEquipmentResourceDoubleBooked { equipment },
        );
    }
    let required = calculate_usable_condition_after_active_ticks(
        profile.condition_wear_ppm_per_active_tick(),
        trace.condition(),
        duration,
    )
    .map_err(PlayerWorkValidationError::ProspectingEquipmentConditionDuration)?;
    if condition_after != required {
        return Err(
            PlayerWorkValidationError::ProspectingEquipmentConditionOutcomeMismatch {
                stored: condition_after,
                required,
            },
        );
    }
    Ok(())
}

fn validate_equipment_replay(
    state: &AppState,
    method: ProspectingDefinition,
    work: ProspectingWork,
) -> Result<(), PlayerWorkValidationError> {
    match (
        method.equipment(),
        work.equipment_trace(),
        work.condition_after(),
    ) {
        (None, None, None) => Ok(()),
        (None, Some(trace), _) => Err(PlayerWorkValidationError::ProspectingUnexpectedEquipment {
            equipment: trace.equipment(),
        }),
        (None, None, Some(_)) | (Some(_), None, _) | (Some(_), Some(_), None) => {
            Err(PlayerWorkValidationError::ProspectingEquipmentMissing)
        }
        (Some(profile), Some(trace), Some(condition_after)) => {
            validate_equipment_trace(state, profile, trace, condition_after, method.duration())
        }
    }
}

fn validate_target_replay(
    registries: &Registries,
    method: ProspectingDefinition,
    work: ProspectingWork,
) -> Result<(), PlayerWorkValidationError> {
    if registries
        .materials()
        .get_material(work.material())
        .is_none()
    {
        return Err(PlayerWorkValidationError::ProspectingUnknownMaterial {
            material: work.material(),
        });
    }
    let region_voxels = work
        .region()
        .voxel_count()
        .ok_or(PlayerWorkValidationError::ProspectingRegionVolumeOverflow)?;
    if region_voxels > method.maximum_region_voxels() {
        return Err(PlayerWorkValidationError::ProspectingRegionTooLarge {
            actual: region_voxels,
            maximum: method.maximum_region_voxels(),
        });
    }
    Ok(())
}

fn validate_schedule_replay(
    state: &AppState,
    method: ProspectingDefinition,
    work: ProspectingWork,
) -> Result<TickSpan, PlayerWorkValidationError> {
    if work.started_at() > state.tick()
        || work.completes_at() <= state.tick()
        || work.completes_at() <= work.started_at()
    {
        return Err(PlayerWorkValidationError::ProspectingScheduleInvalid);
    }
    let stored_duration = TickSpan::new(work.completes_at().value() - work.started_at().value());
    if stored_duration != method.duration() {
        return Err(PlayerWorkValidationError::ProspectingDurationMismatch);
    }
    Ok(TickSpan::new(
        work.completes_at().value() - state.tick().value(),
    ))
}

pub(super) fn validate_prospecting_work(
    registries: &Registries,
    state: &AppState,
    active_jobs: &ActivePlayerJobs,
    work: ProspectingWork,
    available_energy: Energy,
    available_hydration: Volume,
) -> Result<(), PlayerWorkValidationError> {
    if active_jobs.has_any() {
        return Err(PlayerWorkValidationError::MultiplePlayerJobs);
    }
    let method = registries
        .labor()
        .get_prospecting(work.method())
        .copied()
        .ok_or(PlayerWorkValidationError::ProspectingMethodMissing)?;
    validate_equipment_replay(state, method, work)?;
    validate_target_replay(registries, method, work)?;
    let remaining_duration = validate_schedule_replay(state, method, work)?;
    validate_remaining_resources(
        registries,
        available_energy,
        available_hydration,
        method.exertion(),
        remaining_duration,
    )
}
