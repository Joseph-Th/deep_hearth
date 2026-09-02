//! Durable production timing, suspension, and in-flight storage-history validation.

use crate::core::time::SimulationTick;
use crate::inventory::AMBIENT_PRESERVATION_MULTIPLIER_PPM;

use super::super::super::{
    ProductionJobId, ProductionJobRecord, ProductionSuspension, ProductionSuspensionReason,
};
use super::super::ProductionValidationError;

pub(super) fn validate_job_schedule(
    id: ProductionJobId,
    job: &ProductionJobRecord,
    current: SimulationTick,
) -> Result<(), ProductionValidationError> {
    if id.value() == 0 || job.identity.id.value() == 0 {
        return Err(ProductionValidationError::ZeroJobId);
    }
    if id != job.identity.id {
        return Err(ProductionValidationError::JobIdMismatch {
            key: id,
            record: job.identity.id,
        });
    }
    if job.schedule.started_at > current {
        return Err(ProductionValidationError::JobStartedInFuture {
            job: id,
            started_at: job.schedule.started_at,
            current,
        });
    }
    if job.schedule.completes_at <= job.schedule.started_at {
        return Err(ProductionValidationError::CompletionNotAfterStart { job: id });
    }
    match job.schedule.suspension {
        Some(suspension) if suspension.suspended_at() > current => {
            return Err(ProductionValidationError::SuspensionInFuture {
                job: id,
                current,
                suspended_at: suspension.suspended_at(),
            });
        }
        None if job.schedule.completes_at <= current => {
            return Err(ProductionValidationError::RunningJobAlreadyDue {
                job: id,
                due: job.schedule.completes_at,
                current,
            });
        }
        Some(_) | None => {}
    }
    if job.schedule.active_duration.value() == 0 {
        return Err(ProductionValidationError::ZeroActiveDuration { job: id });
    }
    Ok(())
}

pub(super) fn validate_material_storage_history(
    id: ProductionJobId,
    job: &ProductionJobRecord,
    current: SimulationTick,
) -> Result<(), ProductionValidationError> {
    let transition = job.resources.material_storage_history.last_transition_at();
    if transition != job.schedule.started_at {
        return Err(
            ProductionValidationError::StorageHistoryTransitionMismatch {
                job: id,
                transition,
                started_at: job.schedule.started_at,
            },
        );
    }
    job.resources
        .material_storage_history
        .project(current, AMBIENT_PRESERVATION_MULTIPLIER_PPM)
        .ok_or(ProductionValidationError::StorageHistoryOverflow {
            job: id,
            at: current,
        })?;
    job.resources
        .material_storage_history
        .project(
            job.schedule.completes_at,
            AMBIENT_PRESERVATION_MULTIPLIER_PPM,
        )
        .ok_or(ProductionValidationError::StorageHistoryOverflow {
            job: id,
            at: job.schedule.completes_at,
        })?;
    Ok(())
}

pub(super) fn validate_job_suspension(
    id: ProductionJobId,
    job: &ProductionJobRecord,
) -> Result<(), ProductionValidationError> {
    let Some(suspension) = job.schedule.suspension else {
        return Ok(());
    };
    validate_suspension_schedule(id, job, suspension)?;
    validate_suspension_reason(id, job, suspension)
}

pub(super) fn validate_durable_schedule_history(
    id: ProductionJobId,
    job: &ProductionJobRecord,
    current: SimulationTick,
) -> Result<(), ProductionValidationError> {
    let elapsed = current
        .value()
        .checked_sub(job.schedule.started_at.value())
        .unwrap_or_else(|| {
            unreachable!("future production start was rejected before schedule replay")
        });
    if job.schedule.completed_suspension_time.value() > elapsed {
        return Err(
            ProductionValidationError::CompletedSuspensionTimeExceedsElapsed {
                job: id,
                completed: job.schedule.completed_suspension_time,
                elapsed: crate::core::time::TickSpan::new(elapsed),
            },
        );
    }
    let expected_due = job
        .schedule
        .started_at
        .checked_add_span(job.schedule.active_duration)
        .and_then(|base| base.checked_add_span(job.schedule.completed_suspension_time))
        .ok_or(ProductionValidationError::CompletionScheduleOverflow { job: id })?;
    if expected_due != job.schedule.completes_at {
        return Err(ProductionValidationError::CompletionScheduleMismatch {
            job: id,
            expected_due,
            actual_due: job.schedule.completes_at,
        });
    }
    Ok(())
}

fn validate_suspension_schedule(
    id: ProductionJobId,
    job: &ProductionJobRecord,
    suspension: ProductionSuspension,
) -> Result<(), ProductionValidationError> {
    if suspension.remaining_active_time().value() == 0 {
        return Err(ProductionValidationError::ZeroSuspensionRemaining { job: id });
    }
    if suspension.suspended_at() < job.schedule.started_at {
        return Err(ProductionValidationError::SuspensionBeforeStart {
            job: id,
            started_at: job.schedule.started_at,
            suspended_at: suspension.suspended_at(),
        });
    }
    if suspension.remaining_active_time().value() > job.schedule.active_duration.value() {
        return Err(
            ProductionValidationError::SuspensionRemainingExceedsActiveDuration {
                job: id,
                remaining: suspension.remaining_active_time(),
                active_duration: job.schedule.active_duration,
            },
        );
    }
    let expected_due = suspension
        .suspended_at()
        .checked_add_span(suspension.remaining_active_time())
        .ok_or(ProductionValidationError::SuspensionScheduleOverflow { job: id })?;
    if expected_due != job.schedule.completes_at {
        return Err(ProductionValidationError::SuspensionScheduleMismatch {
            job: id,
            expected_due,
            actual_due: job.schedule.completes_at,
        });
    }
    Ok(())
}

fn validate_suspension_reason(
    id: ProductionJobId,
    job: &ProductionJobRecord,
    suspension: ProductionSuspension,
) -> Result<(), ProductionValidationError> {
    match suspension.reason() {
        ProductionSuspensionReason::EquipmentSupportUnavailable { equipment } => {
            if !job.equipment.requires_active_support {
                return Err(
                    ProductionValidationError::SuspensionEquipmentSupportNotRequired { job: id },
                );
            }
            let Some(provider) = job.equipment.provider else {
                return Err(ProductionValidationError::RequiredSupportWithoutEquipment { job: id });
            };
            let expected = provider.equipment();
            if equipment != expected {
                return Err(ProductionValidationError::SuspensionEquipmentMismatch {
                    job: id,
                    expected,
                    reason: equipment,
                });
            }
        }
        ProductionSuspensionReason::OutputSupportUnavailable { stockpile } => {
            if !job
                .output_streams
                .iter()
                .any(|stream| stream.destination == stockpile)
            {
                return Err(ProductionValidationError::SuspensionOutputMismatch {
                    job: id,
                    stockpile,
                });
            }
        }
        ProductionSuspensionReason::PlayerLaborUnavailable => {}
    }
    Ok(())
}
