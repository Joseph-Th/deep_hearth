//! Trusted-load replay for active prospecting work.

use crate::core::quantity::{Energy, Volume};
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::equipment::{EquipmentOccupancy, EquipmentOperationTrace, equipment_occupancy};
use crate::maintenance::{Condition, calculate_usable_condition_after_active_ticks};
use crate::registry::Registries;

use super::{
    ActivePlayerJobs, PlayerWorkValidationError, project_active_work_schedule,
    validate_remaining_resources,
};
use crate::labor::{ProspectingDefinition, ProspectingEquipmentProfile, ProspectingWork};

fn observation_count(
    method: ProspectingDefinition,
    work: ProspectingWork,
) -> Result<u32, PlayerWorkValidationError> {
    match method.spatial_resolution() {
        crate::labor::ProspectingSpatialResolution::AggregateRegion => Ok(1),
        crate::labor::ProspectingSpatialResolution::PerVoxel => work
            .region()
            .voxel_count()
            .and_then(|count| u32::try_from(count).ok())
            .ok_or(PlayerWorkValidationError::ProspectingObservationIdExhausted),
    }
}

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
    let Some(condition_wear_ppm_per_active_tick) =
        profile.condition_wear_ppm_per_active_tick(record.definition())
    else {
        return Err(
            PlayerWorkValidationError::ProspectingEquipmentDefinitionNotAccepted { equipment },
        );
    };
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
        condition_wear_ppm_per_active_tick,
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
    let schedule =
        project_active_work_schedule(state.tick(), work.started_at(), work.completes_at())
            .ok_or(PlayerWorkValidationError::ProspectingScheduleInvalid)?;
    if schedule.duration != method.duration() {
        return Err(PlayerWorkValidationError::ProspectingDurationMismatch);
    }
    Ok(schedule.remaining)
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
    let observations = observation_count(method, work)?;
    if state
        .geological_knowledge()
        .next_observation_id()
        .checked_add(observations)
        .is_none()
    {
        return Err(PlayerWorkValidationError::ProspectingObservationIdExhausted);
    }
    if state
        .geological_knowledge()
        .revision()
        .checked_add(u64::from(observations))
        .is_none()
    {
        return Err(PlayerWorkValidationError::ProspectingKnowledgeRevisionExhausted);
    }
    if work.equipment().is_some() && state.equipment().revision().checked_add(1).is_none() {
        return Err(PlayerWorkValidationError::ProspectingEquipmentRevisionExhausted);
    }
    validate_equipment_replay(state, method, work)?;
    validate_target_replay(registries, method, work)?;
    let remaining_duration = validate_schedule_replay(state, method, work)?;
    validate_remaining_resources(
        registries,
        state,
        available_energy,
        available_hydration,
        method.exertion(),
        remaining_duration,
    )
}
