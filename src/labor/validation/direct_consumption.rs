//! Trusted-load replay validation for persisted eating and drinking attention intervals.

use crate::core::state::AppState;
use crate::core::time::{SimulationTick, TickSpan};
use crate::labor::{DrinkingWork, EatingWork};
use crate::registry::Registries;

use super::{ActivePlayerJobs, PlayerWorkValidationError};

fn validate_schedule(
    current: SimulationTick,
    started_at: SimulationTick,
    completes_at: SimulationTick,
    required_duration: TickSpan,
) -> Option<bool> {
    if started_at > current || completes_at <= current || completes_at <= started_at {
        return None;
    }
    Some(completes_at.value() - started_at.value() == required_duration.value())
}

pub(super) fn validate_eating_work(
    registries: &Registries,
    state: &AppState,
    active_jobs: &ActivePlayerJobs,
    work: EatingWork,
) -> Result<(), PlayerWorkValidationError> {
    if active_jobs.has_any() {
        return Err(PlayerWorkValidationError::MultiplePlayerJobs);
    }
    let required = registries
        .survival()
        .physiology()
        .direct_consumption()
        .meal_duration(work.mass())
        .ok_or(PlayerWorkValidationError::EatingMassInvalid { mass: work.mass() })?;
    match validate_schedule(
        state.tick(),
        work.started_at(),
        work.completes_at(),
        required,
    ) {
        None => Err(PlayerWorkValidationError::EatingScheduleInvalid),
        Some(false) => Err(PlayerWorkValidationError::EatingDurationMismatch),
        Some(true) => Ok(()),
    }
}

pub(super) fn validate_drinking_work(
    registries: &Registries,
    state: &AppState,
    active_jobs: &ActivePlayerJobs,
    work: DrinkingWork,
) -> Result<(), PlayerWorkValidationError> {
    if active_jobs.has_any() {
        return Err(PlayerWorkValidationError::MultiplePlayerJobs);
    }
    let required = registries
        .survival()
        .physiology()
        .direct_consumption()
        .drink_duration(work.volume())
        .ok_or(PlayerWorkValidationError::DrinkingVolumeInvalid {
            volume: work.volume(),
        })?;
    match validate_schedule(
        state.tick(),
        work.started_at(),
        work.completes_at(),
        required,
    ) {
        None => Err(PlayerWorkValidationError::DrinkingScheduleInvalid),
        Some(false) => Err(PlayerWorkValidationError::DrinkingDurationMismatch),
        Some(true) => Ok(()),
    }
}
