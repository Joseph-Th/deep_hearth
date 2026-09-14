//! Validates production-job references, physical traces, output custody, and runtime resources.

use crate::core::state::AppState;
use crate::production::ProductionJobRecord;
use crate::registry::Registries;

use super::StateValidationError;
use super::reservations::ExpectedReservations;

mod matter;
mod resources;
mod subsystems;

use matter::{validate_job_consumed_inputs, validate_job_outputs};
use resources::{
    validate_job_consumed_energy, validate_job_equipment, validate_job_process_and_source,
    validate_job_released_energy, validate_job_resource_topology,
};
use subsystems::validate_job_subsystem_contracts;

pub(super) fn validate_production_references(
    registries: &Registries,
    state: &AppState,
) -> Result<ExpectedReservations, StateValidationError> {
    let mut expected_reservations = ExpectedReservations::default();
    for job in state.systems.production.jobs() {
        validate_production_job(registries, state, job, &mut expected_reservations)?;
    }
    Ok(expected_reservations)
}

fn validate_production_job(
    registries: &Registries,
    state: &AppState,
    job: &ProductionJobRecord,
    expected_reservations: &mut ExpectedReservations,
) -> Result<(), StateValidationError> {
    validate_job_process_and_source(registries, state, job)?;
    validate_job_consumed_energy(registries, state, job)?;
    validate_job_released_energy(registries, state, job)?;
    validate_job_equipment(registries, state, job)?;
    validate_job_resource_topology(registries, job)?;
    validate_job_subsystem_contracts(registries, job)?;
    validate_job_consumed_inputs(registries, job)?;
    validate_job_outputs(registries, state, job, expected_reservations)
}
