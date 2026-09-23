//! Trusted-load replay of production subsystem-specific contracts.

use crate::crafting::validate_loaded_crafting_job;
use crate::ore_processing::{
    validate_loaded_comminution_job, validate_loaded_constituent_separation_job,
    validate_loaded_screening_job,
};
use crate::production::{ProductionJobRecord, ProductionSuspensionReason};
use crate::registry::Registries;
use crate::thermal::validate_loaded_thermal_job;

use super::super::StateValidationError;

pub(super) fn validate_job_subsystem_contracts(
    registries: &Registries,
    job: &ProductionJobRecord,
) -> Result<(), StateValidationError> {
    if job.suspension().is_some_and(|suspension| {
        suspension.reason() == ProductionSuspensionReason::PlayerLaborUnavailable
    }) && registries.manual_process_exertion(job.process()).is_none()
    {
        return Err(StateValidationError::NonManualJobSuspendedForPlayerLabor {
            job: job.id(),
            process: job.process(),
        });
    }
    validate_loaded_comminution_job(registries, job)
        .map_err(StateValidationError::ComminutionJob)?;
    validate_loaded_constituent_separation_job(registries, job)
        .map_err(StateValidationError::ConstituentSeparationJob)?;
    validate_loaded_screening_job(registries, job).map_err(StateValidationError::ScreeningJob)?;
    validate_loaded_thermal_job(registries, job).map_err(StateValidationError::ThermalJob)?;
    validate_loaded_crafting_job(registries, job).map_err(StateValidationError::CraftingJob)
}
