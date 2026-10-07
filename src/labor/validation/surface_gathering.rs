//! Trusted-load replay validation for active loose surface gathering.

use crate::core::quantity::{Energy, Volume};
use crate::core::state::AppState;
use crate::inventory::validate_stockpile_storage;
use crate::labor::SurfaceGatheringWork;
use crate::logistics::validate_player_stockpile_access;
use crate::registry::Registries;
use crate::surface::SurfaceResourceLifecycle;

use super::{
    ActivePlayerJobs, PlayerWorkValidationError, project_active_work_schedule,
    validate_remaining_resources,
};

pub(super) fn validate_surface_gathering_work(
    registries: &Registries,
    state: &AppState,
    active_jobs: &ActivePlayerJobs,
    work: SurfaceGatheringWork,
    available_energy: Energy,
    available_hydration: Volume,
) -> Result<(), PlayerWorkValidationError> {
    if active_jobs.has_any() {
        return Err(PlayerWorkValidationError::MultiplePlayerJobs);
    }
    let method = registries
        .labor()
        .get_surface_gathering(work.method())
        .copied()
        .ok_or(PlayerWorkValidationError::SurfaceGatheringMethodMissing)?;
    let resource = state
        .surface()
        .get(work.resource())
        .ok_or(PlayerWorkValidationError::SurfaceGatheringResourceMissing)?;
    let player_position = state
        .logistics()
        .player()
        .map(|player| player.position())
        .ok_or(PlayerWorkValidationError::SurfaceGatheringPlayerNotLocated)?;
    if player_position != resource.position() {
        return Err(PlayerWorkValidationError::SurfaceGatheringPlayerRemote);
    }
    if resource.commodity() != method.commodity() {
        return Err(
            PlayerWorkValidationError::SurfaceGatheringCommodityMismatch {
                expected: method.commodity(),
                actual: resource.commodity(),
            },
        );
    }
    if resource.lifecycle() != SurfaceResourceLifecycle::Available {
        return Err(PlayerWorkValidationError::SurfaceGatheringResourceDepleted);
    }
    if resource.remaining_mass() != work.source_mass_before()
        || resource.remaining_mass() < work.gathered_mass()
    {
        return Err(PlayerWorkValidationError::SurfaceGatheringSourceMassMismatch);
    }
    if work.gathered_mass().is_zero() || work.gathered_mass() > method.maximum_batch_mass() {
        return Err(PlayerWorkValidationError::SurfaceGatheringMassInvalid);
    }
    validate_player_stockpile_access(state, work.destination())
        .map_err(PlayerWorkValidationError::SurfaceGatheringDestinationAccess)?;
    let destination = state
        .inventory()
        .get_stockpile(work.destination())
        .ok_or(PlayerWorkValidationError::SurfaceGatheringDestinationMissing)?;
    if destination.supported_by().is_some() {
        return Err(PlayerWorkValidationError::SurfaceGatheringDestinationMounted);
    }
    validate_stockpile_storage(
        registries,
        destination,
        work.destination(),
        resource.commodity(),
        resource.composition(),
        resource.temperature(),
        None,
    )
    .map_err(PlayerWorkValidationError::SurfaceGatheringDestinationStorage)?;
    if destination.reserved_inbound() < work.gathered_mass() {
        return Err(PlayerWorkValidationError::SurfaceGatheringReservationMissing);
    }
    let schedule =
        project_active_work_schedule(state.tick(), work.started_at(), work.completes_at())
            .ok_or(PlayerWorkValidationError::SurfaceGatheringScheduleInvalid)?;
    let required_duration = method
        .duration_for_mass(work.gathered_mass())
        .ok_or(PlayerWorkValidationError::SurfaceGatheringMassInvalid)?;
    if schedule.duration != required_duration {
        return Err(PlayerWorkValidationError::SurfaceGatheringDurationMismatch);
    }
    validate_remaining_resources(
        registries,
        state,
        available_energy,
        available_hydration,
        method.exertion(),
        schedule.remaining,
    )
}
