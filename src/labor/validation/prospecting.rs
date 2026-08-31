//! Trusted-load replay for active prospecting work.

use crate::core::quantity::{Energy, Volume};
use crate::core::state::AppState;
use crate::core::time::TickSpan;
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
