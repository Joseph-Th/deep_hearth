//! Validates persisted production jobs, schedules, streams, and derived indexes.

use crate::core::time::SimulationTick;

use super::ProductionState;

mod error;
mod indexes;
mod job;

pub use error::ProductionValidationError;

use indexes::{
    validate_due_index, validate_job_due_membership, validate_job_id_cursor,
    validate_occupancy_indexes,
};
use job::{validate_durable_schedule_history, validate_job_record};

pub(crate) fn validate_loaded_production(
    state: &ProductionState,
    current: SimulationTick,
) -> Result<(), ProductionValidationError> {
    validate_job_id_cursor(state)?;
    for (id, job) in &state.jobs {
        validate_job_record(*id, job, current)?;
        validate_job_due_membership(state, *id, job)?;
    }
    validate_due_index(state)?;
    validate_occupancy_indexes(state)
}

/// Replays the durable wall-clock schedule after operation-specific validators have established
/// that each job's persisted active duration matches its physical process contract.
pub(crate) fn validate_loaded_production_schedule_history(
    state: &ProductionState,
    current: SimulationTick,
) -> Result<(), ProductionValidationError> {
    for (id, job) in &state.jobs {
        validate_durable_schedule_history(*id, job, current)?;
    }
    Ok(())
}
