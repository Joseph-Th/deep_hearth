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
    JobIdMismatch {
        key: MiningJobId,
        record: MiningJobId,
    },
    JobIdBeyondCursor {
        job: MiningJobId,
    },
    ZeroOutputMass {
        job: MiningJobId,
    },
    JobStartedInFuture {
        job: MiningJobId,
        started: SimulationTick,
        current: SimulationTick,
    },
    CompletionNotAfterStart {
        job: MiningJobId,
        started: SimulationTick,
        completes: SimulationTick,
    },
    WorkingJobAlreadyDue {
        job: MiningJobId,
        due: SimulationTick,
        current: SimulationTick,
    },
    ReadyTickMismatch {
        job: MiningJobId,
        ready: SimulationTick,
        due: SimulationTick,
    },
    ReadyInFuture {
        job: MiningJobId,
        ready: SimulationTick,
        current: SimulationTick,
    },
    DueIndexMismatch,
    EquipmentOccupancyMismatch,
    EquipmentDoubleBooked {
        equipment: EquipmentId,
    },
}

impl Display for MiningValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidIdCursor => formatter.write_str("mining job identifier cursor is invalid"),
            Self::JobIdMismatch { key, record } => write!(
                formatter,
                "mining job map key {} disagrees with record id {}",
                key.value(),
                record.value()
            ),
            Self::JobIdBeyondCursor { job } => write!(
                formatter,
                "mining job {} is not below the next identifier cursor",
                job.value()
            ),
            Self::ZeroOutputMass { job } => {
                write!(formatter, "mining job {} has zero output mass", job.value())
            }
            Self::JobStartedInFuture {
                job,
                started,
                current,
            } => write!(
                formatter,
                "mining job {} starts at tick {} after current tick {}",
                job.value(),
                started.value(),
                current.value()
            ),
            Self::CompletionNotAfterStart {
                job,
                started,
                completes,
            } => write!(
                formatter,
                "mining job {} completes at tick {} but starts at tick {}",
                job.value(),
                completes.value(),
                started.value()
            ),
            Self::WorkingJobAlreadyDue { job, due, current } => write!(
                formatter,
                "active mining job {} was due at tick {} by current tick {}",
                job.value(),
                due.value(),
                current.value()
            ),
            Self::ReadyTickMismatch { job, ready, due } => write!(
                formatter,
                "ready mining job {} records ready tick {} but completion tick is {}",
                job.value(),
                ready.value(),
                due.value()
            ),
            Self::ReadyInFuture {
                job,
                ready,
                current,
            } => write!(
                formatter,
                "mining job {} is marked ready at future tick {} from current tick {}",
                job.value(),
                ready.value(),
                current.value()
            ),
            Self::DueIndexMismatch => {
                formatter.write_str("mining due-job index does not match jobs")
            }
            Self::EquipmentOccupancyMismatch => {
                formatter.write_str("mining equipment occupancy index does not match active jobs")
            }
            Self::EquipmentDoubleBooked { equipment } => write!(
                formatter,
                "mining equipment {} is assigned to more than one active job",
                equipment.value()
            ),
        }
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
    for (key, job) in &state.jobs {
        if *key != job.identity.id {
            return Err(MiningValidationError::JobIdMismatch {
                key: *key,
                record: job.identity.id,
            });
        }
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
        if job.schedule.started_at > current {
            return Err(MiningValidationError::JobStartedInFuture {
                job: job.identity.id,
                started: job.schedule.started_at,
                current,
            });
        }
        if job.schedule.completes_at <= job.schedule.started_at {
            return Err(MiningValidationError::CompletionNotAfterStart {
                job: job.identity.id,
                started: job.schedule.started_at,
                completes: job.schedule.completes_at,
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
                    .insert(job.equipment(), job.identity.id)
                    .is_some()
                {
                    return Err(MiningValidationError::EquipmentDoubleBooked {
                        equipment: job.equipment(),
                    });
                }
            }
            Some(ready) => {
                if ready != job.schedule.completes_at {
                    return Err(MiningValidationError::ReadyTickMismatch {
                        job: job.identity.id,
                        ready,
                        due: job.schedule.completes_at,
                    });
                }
                if ready > current {
                    return Err(MiningValidationError::ReadyInFuture {
                        job: job.identity.id,
                        ready,
                        current,
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
