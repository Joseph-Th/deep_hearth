//! Exhaustive persistence validation for mining jobs and synchronized runtime indexes.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::time::SimulationTick;
use crate::equipment::EquipmentId;

use super::{MiningJobId, MiningJobRecord, MiningState};

#[derive(Default)]
struct ExpectedMiningIndexes {
    due: BTreeMap<SimulationTick, BTreeSet<MiningJobId>>,
    equipment: BTreeMap<EquipmentId, MiningJobId>,
}

impl ExpectedMiningIndexes {
    fn add_working_job(&mut self, job: &MiningJobRecord) -> Result<(), MiningValidationError> {
        self.due
            .entry(job.completes_at())
            .or_default()
            .insert(job.id());
        if self.equipment.insert(job.equipment(), job.id()).is_some() {
            return Err(MiningValidationError::EquipmentDoubleBooked {
                equipment: job.equipment(),
            });
        }
        Ok(())
    }
}

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
    ReadyClaimOutsideCompletionTick {
        job: MiningJobId,
        completion: SimulationTick,
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
            Self::ReadyClaimOutsideCompletionTick {
                job,
                completion,
                current,
            } => write!(
                formatter,
                "ready mining job {} must be claimed at completion tick {} but current tick is {}",
                job.value(),
                completion.value(),
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
    let mut expected = ExpectedMiningIndexes::default();
    for (key, job) in &state.jobs {
        validate_mining_job(state, *key, job, current, &mut expected)?;
    }
    if state.due_jobs != expected.due {
        return Err(MiningValidationError::DueIndexMismatch);
    }
    if state.equipment_occupancy != expected.equipment {
        return Err(MiningValidationError::EquipmentOccupancyMismatch);
    }
    Ok(())
}

fn validate_mining_job(
    state: &MiningState,
    key: MiningJobId,
    job: &MiningJobRecord,
    current: SimulationTick,
    expected: &mut ExpectedMiningIndexes,
) -> Result<(), MiningValidationError> {
    validate_mining_job_identity(state, key, job)?;
    validate_mining_job_schedule(job, current)?;
    if job.is_working() {
        validate_working_mining_job(job, current, expected)
    } else {
        validate_ready_mining_job(job, current)
    }
}

fn validate_mining_job_identity(
    state: &MiningState,
    key: MiningJobId,
    job: &MiningJobRecord,
) -> Result<(), MiningValidationError> {
    if key != job.id() {
        return Err(MiningValidationError::JobIdMismatch {
            key,
            record: job.id(),
        });
    }
    if job.id().value() >= state.next_job_id {
        return Err(MiningValidationError::JobIdBeyondCursor { job: job.id() });
    }
    if job.output().mass().is_zero() {
        return Err(MiningValidationError::ZeroOutputMass { job: job.id() });
    }
    Ok(())
}

fn validate_mining_job_schedule(
    job: &MiningJobRecord,
    current: SimulationTick,
) -> Result<(), MiningValidationError> {
    if job.started_at() > current {
        return Err(MiningValidationError::JobStartedInFuture {
            job: job.id(),
            started: job.started_at(),
            current,
        });
    }
    if job.completes_at() <= job.started_at() {
        return Err(MiningValidationError::CompletionNotAfterStart {
            job: job.id(),
            started: job.started_at(),
            completes: job.completes_at(),
        });
    }
    Ok(())
}

fn validate_working_mining_job(
    job: &MiningJobRecord,
    current: SimulationTick,
    expected: &mut ExpectedMiningIndexes,
) -> Result<(), MiningValidationError> {
    if job.completes_at() <= current {
        return Err(MiningValidationError::WorkingJobAlreadyDue {
            job: job.id(),
            due: job.completes_at(),
            current,
        });
    }
    expected.add_working_job(job)
}

fn validate_ready_mining_job(
    job: &MiningJobRecord,
    current: SimulationTick,
) -> Result<(), MiningValidationError> {
    if job.completes_at() != current {
        return Err(MiningValidationError::ReadyClaimOutsideCompletionTick {
            job: job.id(),
            completion: job.completes_at(),
            current,
        });
    }
    Ok(())
}
