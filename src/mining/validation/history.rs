//! Retained mining-history validation across current durable job records.

use std::collections::BTreeMap;

use crate::core::state::AppState;
use crate::geology::GeologicalDepositId;

use super::MiningJobValidationError;
use crate::mining::MiningJobRecord;

pub(super) fn validate_retained_work_intervals(
    state: &AppState,
) -> Result<(), MiningJobValidationError> {
    let mut jobs = state.mining().jobs().collect::<Vec<_>>();
    jobs.sort_by_key(|job| (job.started_at(), job.id()));
    for pair in jobs.windows(2) {
        let earlier = pair[0];
        let later = pair[1];
        if later.started_at() < earlier.completes_at() {
            return Err(MiningJobValidationError::OverlappingRetainedWork {
                earlier: earlier.id(),
                later: later.id(),
                earlier_completes: earlier.completes_at(),
                later_starts: later.started_at(),
            });
        }
    }
    Ok(())
}

pub(super) fn validate_retained_deposit_history(
    state: &AppState,
) -> Result<(), MiningJobValidationError> {
    let mut by_deposit = BTreeMap::<GeologicalDepositId, Vec<&MiningJobRecord>>::new();
    for job in state.mining().jobs() {
        by_deposit.entry(job.deposit()).or_default().push(job);
    }
    for jobs in by_deposit.values_mut() {
        jobs.sort_by_key(|job| (job.completes_at(), job.id()));
        for pair in jobs.windows(2) {
            let earlier = pair[0];
            let later = pair[1];
            let maximum_later_mass = earlier
                .deposit_mass_before()
                .checked_sub(earlier.output().mass())
                .unwrap_or_else(|| {
                    unreachable!(
                        "individual mining source validation ran before history validation"
                    )
                });
            if later.deposit_mass_before() > maximum_later_mass {
                return Err(MiningJobValidationError::DepositHistoryMassIncrease {
                    earlier: earlier.id(),
                    later: later.id(),
                    maximum_later_mass,
                    later_mass: later.deposit_mass_before(),
                });
            }
        }
    }
    Ok(())
}
