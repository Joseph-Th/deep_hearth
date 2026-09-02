//! Trusted-load replay for active prospecting work.

use crate::core::quantity::{Energy, Volume};
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::maintenance::calculate_usable_condition_after_active_ticks;
use crate::registry::Registries;

use super::{ActivePlayerJobs, PlayerWorkValidationError, validate_remaining_resources};
use crate::labor::ProspectingWork;

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
    match (
        method.equipment(),
        work.equipment_trace(),
        work.condition_after(),
    ) {
        (None, None, None) => {}
        (None, Some(trace), _) => {
            return Err(PlayerWorkValidationError::ProspectingUnexpectedEquipment {
                equipment: trace.equipment(),
            });
        }
        (None, None, Some(_)) => {
            return Err(PlayerWorkValidationError::ProspectingEquipmentMissing);
        }
        (Some(_), None, _) | (Some(_), Some(_), None) => {
            return Err(PlayerWorkValidationError::ProspectingEquipmentMissing);
        }
        (Some(profile), Some(trace), Some(condition_after)) => {
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
                    PlayerWorkValidationError::ProspectingEquipmentDefinitionNotAccepted {
                        equipment,
                    },
                );
            }
            if record.condition() != trace.condition() {
                return Err(PlayerWorkValidationError::ProspectingEquipmentConditionMismatch);
            }
            if record.supported_by().is_some() {
                return Err(PlayerWorkValidationError::ProspectingEquipmentMounted { equipment });
            }
            if state
                .production()
                .get_equipment_occupant(equipment)
                .is_some()
                || state.mining().get_equipment_occupant(equipment).is_some()
            {
                return Err(
                    PlayerWorkValidationError::ProspectingEquipmentResourceDoubleBooked {
                        equipment,
                    },
                );
            }
            let required = calculate_usable_condition_after_active_ticks(
                profile.condition_wear_ppm_per_active_tick(),
                trace.condition(),
                method.duration(),
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
        }
    }
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
    let remaining_ticks = work.completes_at().value() - state.tick().value();
    validate_remaining_resources(
        registries,
        available_energy,
        available_hydration,
        method.exertion(),
        TickSpan::new(remaining_ticks),
    )
}
