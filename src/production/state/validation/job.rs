//! Durable production-job record validation, independent of derived owner indexes.

use crate::core::time::SimulationTick;

use super::super::{ProductionJobId, ProductionJobRecord};
use super::ProductionValidationError;

mod outputs;
mod resources;
mod schedule;

use outputs::validate_outputs;
use resources::{validate_consumed_resources, validate_energy_traces, validate_equipment_outcome};
use schedule::{validate_job_schedule, validate_job_suspension, validate_material_storage_history};

pub(super) fn validate_durable_schedule_history(
    id: ProductionJobId,
    job: &ProductionJobRecord,
    current: SimulationTick,
) -> Result<(), ProductionValidationError> {
    schedule::validate_durable_schedule_history(id, job, current)
}

pub(super) fn validate_job_record(
    id: ProductionJobId,
    job: &ProductionJobRecord,
    current: SimulationTick,
) -> Result<(), ProductionValidationError> {
    validate_job_schedule(id, job, current)?;
    validate_job_suspension(id, job)?;
    validate_material_storage_history(id, job, current)?;
    let consumed_mass = validate_consumed_resources(id, job)?;
    validate_energy_traces(id, job)?;
    validate_equipment_outcome(id, job)?;
    validate_outputs(id, job, consumed_mass)
}
