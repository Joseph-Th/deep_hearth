//! Exhaustive persistence validation for mining jobs and synchronized runtime indexes.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::time::SimulationTick;
use crate::equipment::EquipmentId;

use super::{MiningJobId, MiningState};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MiningValidationError {
    InvalidIdCursor,
    JobIdBeyondCursor {
        job: MiningJobId,
    },
    ZeroOutputMass {
        job: MiningJobId,
    },
    WorkingJobAlreadyDue {
        job: MiningJobId,
        due: SimulationTick,
        current: SimulationTick,
    },
    ReadyBeforeCompletion {
        job: MiningJobId,
        ready: SimulationTick,
        due: SimulationTick,
    },
    DueIndexMismatch,
    EquipmentOccupancyMismatch,
    EquipmentDoubleBooked {
        equipment: EquipmentId,
    },
}

impl Display for MiningValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid mining state: {self:?}")
    }
}

impl Error for MiningValidationError {}

pub(crate) fn validate_loaded_mining(
    state: &MiningState,
    current: SimulationTick,
) -> Result<(), MiningValidationError> {
    if !state.has_valid_id_cursor() {
        return Err(MiningValidationError::InvalidIdCursor);
    }
    let mut expected_due = BTreeMap::<SimulationTick, BTreeSet<MiningJobId>>::new();
    let mut expected_equipment = BTreeMap::<EquipmentId, MiningJobId>::new();
    for job in state.jobs.values() {
        if job.identity.id.value() >= state.next_job_id {
            return Err(MiningValidationError::JobIdBeyondCursor {
                job: job.identity.id,
            });
        }
        if job.resources.output.mass().is_zero() {
            return Err(MiningValidationError::ZeroOutputMass {
                job: job.identity.id,
            });
        }
        match job.schedule.ready_at {
            None => {
                if job.schedule.completes_at <= current {
                    return Err(MiningValidationError::WorkingJobAlreadyDue {
                        job: job.identity.id,
                        due: job.schedule.completes_at,
                        current,
                    });
                }
                expected_due
                    .entry(job.schedule.completes_at)
                    .or_default()
                    .insert(job.identity.id);
                if expected_equipment
                    .insert(job.resources.equipment, job.identity.id)
                    .is_some()
                {
                    return Err(MiningValidationError::EquipmentDoubleBooked {
                        equipment: job.resources.equipment,
                    });
                }
            }
            Some(ready) => {
                if ready < job.schedule.completes_at {
                    return Err(MiningValidationError::ReadyBeforeCompletion {
                        job: job.identity.id,
                        ready,
                        due: job.schedule.completes_at,
                    });
                }
            }
        }
    }
    if state.due_jobs != expected_due {
        return Err(MiningValidationError::DueIndexMismatch);
    }
    if state.equipment_occupancy != expected_equipment {
        return Err(MiningValidationError::EquipmentOccupancyMismatch);
    }
    Ok(())
}
