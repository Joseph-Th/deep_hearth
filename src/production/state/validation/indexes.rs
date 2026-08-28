//! Production derived-index validation against durable job records.

use crate::core::time::SimulationTick;

use super::super::{ProductionJobId, ProductionJobRecord, ProductionState};
use super::ProductionValidationError;

pub(super) fn validate_job_id_cursor(
    state: &ProductionState,
) -> Result<(), ProductionValidationError> {
    if state.next_job_id == 0 {
        return Err(ProductionValidationError::ZeroNextJobId);
    }
    if let Some(highest) = state.jobs.keys().next_back().copied()
        && state.next_job_id <= highest.value()
    {
        return Err(ProductionValidationError::NextIdNotAfterExisting {
            next: state.next_job_id,
            highest,
        });
    }
    Ok(())
}

pub(super) fn validate_job_due_membership(
    state: &ProductionState,
    id: ProductionJobId,
    job: &ProductionJobRecord,
) -> Result<(), ProductionValidationError> {
    let is_indexed = state
        .due_jobs
        .get(&job.schedule.completes_at)
        .is_some_and(|ids| ids.contains(&id));
    if job.schedule.suspension.is_some() && is_indexed {
        return Err(ProductionValidationError::SuspendedJobInDueIndex {
            job: id,
            due: job.schedule.completes_at,
        });
    }
    if job.schedule.suspension.is_none() && !is_indexed {
        return Err(ProductionValidationError::MissingDueIndex {
            job: id,
            due: job.schedule.completes_at,
        });
    }
    Ok(())
}

pub(super) fn validate_due_index(state: &ProductionState) -> Result<(), ProductionValidationError> {
    for (due, ids) in &state.due_jobs {
        if ids.is_empty() {
            return Err(ProductionValidationError::EmptyDueIndex { due: *due });
        }
        for id in ids {
            validate_due_index_entry(state, *due, *id)?;
        }
    }
    Ok(())
}

fn validate_due_index_entry(
    state: &ProductionState,
    due: SimulationTick,
    id: ProductionJobId,
) -> Result<(), ProductionValidationError> {
    let Some(job) = state.jobs.get(&id) else {
        return Err(ProductionValidationError::UnexpectedDueIndex { job: id, due });
    };
    if job.schedule.suspension.is_some() {
        return Err(ProductionValidationError::SuspendedJobInDueIndex { job: id, due });
    }
    if job.schedule.completes_at != due {
        return Err(ProductionValidationError::UnexpectedDueIndex { job: id, due });
    }
    Ok(())
}

pub(super) fn validate_occupancy_indexes(
    state: &ProductionState,
) -> Result<(), ProductionValidationError> {
    if let Some((store, indexed, expected)) = state
        .energy_occupancy_mismatch()
        .map_err(|store| ProductionValidationError::EnergyDoubleBooked { store })?
    {
        return Err(ProductionValidationError::EnergyOccupancyIndexMismatch {
            store,
            indexed,
            expected,
        });
    }
    if let Some((equipment, indexed, expected)) = state
        .equipment_occupancy_mismatch()
        .map_err(|equipment| ProductionValidationError::EquipmentDoubleBooked { equipment })?
    {
        return Err(ProductionValidationError::EquipmentOccupancyIndexMismatch {
            equipment,
            indexed,
            expected,
        });
    }
    if let Some(stockpile) = state.output_stockpile_occupancy_mismatch() {
        return Err(ProductionValidationError::OutputStockpileOccupancyIndexMismatch { stockpile });
    }
    Ok(())
}
