//! Trusted-load replay validation for persisted eating and drinking attention intervals.

use crate::core::state::AppState;
use crate::labor::{DrinkingWork, EatingWork, PlayerWork};
use crate::registry::Registries;
use crate::survival::PendingDirectConsumption;

use super::{ActivePlayerJobs, PlayerWorkValidationError, project_active_work_schedule};

fn validate_eating_binding(
    work: EatingWork,
    pending: Option<&PendingDirectConsumption>,
) -> Result<(), PlayerWorkValidationError> {
    match pending {
        None => Err(PlayerWorkValidationError::EatingConsumptionMissing),
        Some(PendingDirectConsumption::Eating(pending))
            if pending.total_mass() == Some(work.mass())
                && pending.started_at() == work.started_at()
                && pending.completes_at() == work.completes_at() =>
        {
            Ok(())
        }
        Some(_) => Err(PlayerWorkValidationError::EatingConsumptionMismatch),
    }
}

fn validate_drinking_binding(
    work: DrinkingWork,
    pending: Option<&PendingDirectConsumption>,
) -> Result<(), PlayerWorkValidationError> {
    match pending {
        None => Err(PlayerWorkValidationError::DrinkingConsumptionMissing),
        Some(PendingDirectConsumption::Drinking(pending))
            if pending.volume() == work.volume()
                && pending.started_at() == work.started_at()
                && pending.completes_at() == work.completes_at() =>
        {
            Ok(())
        }
        Some(_) => Err(PlayerWorkValidationError::DrinkingConsumptionMismatch),
    }
}

pub(super) fn validate_direct_consumption_binding(
    state: &AppState,
    active: Option<PlayerWork>,
) -> Result<(), PlayerWorkValidationError> {
    let pending = state.survival().pending_direct_consumption();
    match active {
        Some(PlayerWork::Eating { work }) => validate_eating_binding(work, pending),
        Some(PlayerWork::Drinking { work }) => validate_drinking_binding(work, pending),
        Some(
            PlayerWork::ManualProduction { .. }
            | PlayerWork::Mining { .. }
            | PlayerWork::ManualPower { .. }
            | PlayerWork::Prospecting { .. }
            | PlayerWork::EquipmentMaintenance { .. }
            | PlayerWork::StorageEnclosureDismantling { .. },
        )
        | None
            if pending.is_none() =>
        {
            Ok(())
        }
        Some(
            PlayerWork::ManualProduction { .. }
            | PlayerWork::Mining { .. }
            | PlayerWork::ManualPower { .. }
            | PlayerWork::Prospecting { .. }
            | PlayerWork::EquipmentMaintenance { .. }
            | PlayerWork::StorageEnclosureDismantling { .. },
        )
        | None => Err(PlayerWorkValidationError::PendingDirectConsumptionWithoutWork),
    }
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
    let schedule =
        project_active_work_schedule(state.tick(), work.started_at(), work.completes_at())
            .ok_or(PlayerWorkValidationError::EatingScheduleInvalid)?;
    if schedule.duration != required {
        return Err(PlayerWorkValidationError::EatingDurationMismatch);
    }
    Ok(())
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
    let schedule =
        project_active_work_schedule(state.tick(), work.started_at(), work.completes_at())
            .ok_or(PlayerWorkValidationError::DrinkingScheduleInvalid)?;
    if schedule.duration != required {
        return Err(PlayerWorkValidationError::DrinkingDurationMismatch);
    }
    Ok(())
}
