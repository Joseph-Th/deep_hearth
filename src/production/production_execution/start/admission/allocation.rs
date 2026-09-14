//! Production job identity and schedule admission.

use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::production::{ProcessResolution, ProductionJobId};

use super::super::StartProcessError;

#[must_use]
pub(in super::super) struct ValidatedJobAllocation {
    pub(in super::super) current: SimulationTick,
    pub(in super::super) completes_at: SimulationTick,
    pub(in super::super) job_id: ProductionJobId,
    pub(in super::super) next_job_id: u64,
    pub(in super::super) expected_production_revision: u64,
    pub(in super::super) next_production_revision: u64,
}

pub(in super::super) fn validate_job_allocation(
    state: &AppState,
    resolution: &ProcessResolution,
) -> Result<ValidatedJobAllocation, StartProcessError> {
    let current = state.tick();
    let completes_at = current.checked_add_span(resolution.duration()).ok_or(
        StartProcessError::CompletionTickOverflow {
            current,
            duration_ticks: resolution.duration().value(),
        },
    )?;
    let job_value = state.production().next_job_id();
    let next_job_id = job_value
        .checked_add(1)
        .ok_or(StartProcessError::JobIdExhausted)?;
    let expected_production_revision = state.production().revision();
    // Admission inserts the durable job and its scheduled completion removes or transitions it.
    // Require both deterministic owner mutations up front so starting a job cannot consume the
    // final production revision and make its first due transition impossible.
    expected_production_revision
        .checked_add(2)
        .ok_or(StartProcessError::ProductionRevisionExhausted)?;
    let next_production_revision = expected_production_revision
        .checked_add(1)
        .unwrap_or_else(|| unreachable!("two-step production revision budget includes admission"));
    Ok(ValidatedJobAllocation {
        current,
        completes_at,
        job_id: ProductionJobId::new(job_value),
        next_job_id,
        expected_production_revision,
        next_production_revision,
    })
}
