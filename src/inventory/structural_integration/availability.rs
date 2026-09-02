//! Runtime occupancy policy for stockpile support relocation.

use crate::core::state::AppState;
use crate::inventory::StockpileId;
use crate::production::{ProductionJobId, ProductionOccupancyRelease};

use super::{StockpileSupportCommitError, StockpileSupportError};

/// Current runtime ownership that prevents physically moving a stockpile support assignment.
///
/// Only running production blocks output-destination relocation. Suspended production keeps its
/// inbound reservation but deliberately permits support recovery before the job resumes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StockpileSupportBlocker {
    ProductionOutput {
        job: ProductionJobId,
        release: ProductionOccupancyRelease,
    },
    StorageDismantling,
}

fn blocker(state: &AppState, stockpile: StockpileId) -> Option<StockpileSupportBlocker> {
    if let Some(job) = state
        .production()
        .get_running_output_stockpile_occupant(stockpile)
    {
        return Some(StockpileSupportBlocker::ProductionOutput {
            job: job.id(),
            release: job.occupancy_release(),
        });
    }
    state
        .player_work()
        .get_storage_dismantling_stockpile_occupant(stockpile)
        .is_some()
        .then_some(StockpileSupportBlocker::StorageDismantling)
}

pub(super) fn support_validation_error(
    state: &AppState,
    stockpile: StockpileId,
) -> Option<StockpileSupportError> {
    blocker(state, stockpile).map(|blocker| match blocker {
        StockpileSupportBlocker::ProductionOutput { job, release } => {
            StockpileSupportError::StockpileBusy {
                stockpile,
                job,
                release,
            }
        }
        StockpileSupportBlocker::StorageDismantling => {
            StockpileSupportError::StockpileBusyStorageDismantling { stockpile }
        }
    })
}

pub(super) fn support_commit_error(
    state: &AppState,
    stockpile: StockpileId,
) -> Option<StockpileSupportCommitError> {
    blocker(state, stockpile).map(|blocker| match blocker {
        StockpileSupportBlocker::ProductionOutput { job, release } => {
            StockpileSupportCommitError::StockpileBusy {
                stockpile,
                job,
                release,
            }
        }
        StockpileSupportBlocker::StorageDismantling => {
            StockpileSupportCommitError::StockpileBusyStorageDismantling { stockpile }
        }
    })
}
