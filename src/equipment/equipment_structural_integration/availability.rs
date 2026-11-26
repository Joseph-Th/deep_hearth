//! Runtime occupancy policy for equipment support relocation.

use crate::core::state::AppState;
use crate::core::time::{ControlTick, SimulationTick};
use crate::equipment::{EquipmentId, EquipmentOccupancy, equipment_occupancy};
use crate::mining::MiningJobId;
use crate::production::{ProductionJobId, ProductionOccupancyRelease};

use super::{EquipmentSupportCommitError, EquipmentSupportError};

/// Current runtime ownership that prevents physically changing an equipment support assignment.
///
/// A suspended support-dependent job permits support recovery. A freestanding job retains its
/// original support mode until completion, even while paused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EquipmentSupportBlocker {
    Production {
        job: ProductionJobId,
        release: ProductionOccupancyRelease,
    },
    Mining {
        job: MiningJobId,
    },
    ManualPower,
    Prospecting {
        completes_at: SimulationTick,
    },
    BlockExcavation {
        completes_at: ControlTick,
    },
    TreeFelling {
        completes_at: SimulationTick,
    },
    Maintenance {
        completes_at: SimulationTick,
    },
}

fn blocker(state: &AppState, equipment: EquipmentId) -> Option<EquipmentSupportBlocker> {
    equipment_occupancy(state, equipment).and_then(|occupancy| match occupancy {
        EquipmentOccupancy::Production { job, release } => {
            let permits_recovery = release == ProductionOccupancyRelease::AwaitingResume
                && state
                    .production()
                    .get_job(job)
                    .is_some_and(|record| record.has_required_active_support());
            (!permits_recovery).then_some(EquipmentSupportBlocker::Production { job, release })
        }
        EquipmentOccupancy::Mining { job } => Some(EquipmentSupportBlocker::Mining { job }),
        EquipmentOccupancy::ManualPower { .. } => Some(EquipmentSupportBlocker::ManualPower),
        EquipmentOccupancy::Prospecting { completes_at } => {
            Some(EquipmentSupportBlocker::Prospecting { completes_at })
        }
        EquipmentOccupancy::TreeFelling { completes_at } => {
            Some(EquipmentSupportBlocker::TreeFelling { completes_at })
        }
        EquipmentOccupancy::BlockExcavation { completes_at } => {
            Some(EquipmentSupportBlocker::BlockExcavation { completes_at })
        }
        EquipmentOccupancy::Maintenance { completes_at } => {
            Some(EquipmentSupportBlocker::Maintenance { completes_at })
        }
    })
}

pub(super) fn support_validation_error(
    state: &AppState,
    equipment: EquipmentId,
) -> Option<EquipmentSupportError> {
    blocker(state, equipment).map(|blocker| match blocker {
        EquipmentSupportBlocker::Production { job, release } => {
            EquipmentSupportError::EquipmentBusy {
                equipment,
                job,
                release,
            }
        }
        EquipmentSupportBlocker::Mining { job } => {
            EquipmentSupportError::EquipmentBusyMining { equipment, job }
        }
        EquipmentSupportBlocker::ManualPower => {
            EquipmentSupportError::EquipmentBusyManualPower { equipment }
        }
        EquipmentSupportBlocker::Prospecting { completes_at } => {
            EquipmentSupportError::EquipmentBusyProspecting {
                equipment,
                completes_at,
            }
        }
        EquipmentSupportBlocker::TreeFelling { completes_at } => {
            EquipmentSupportError::EquipmentBusyTreeFelling {
                equipment,
                completes_at,
            }
        }
        EquipmentSupportBlocker::BlockExcavation { completes_at } => {
            EquipmentSupportError::EquipmentBusyBlockExcavation {
                equipment,
                completes_at,
            }
        }
        EquipmentSupportBlocker::Maintenance { completes_at } => {
            EquipmentSupportError::EquipmentUnderMaintenance {
                equipment,
                completes_at,
            }
        }
    })
}

pub(super) fn support_commit_error(
    state: &AppState,
    equipment: EquipmentId,
) -> Option<EquipmentSupportCommitError> {
    blocker(state, equipment).map(|blocker| match blocker {
        EquipmentSupportBlocker::Production { job, release } => {
            EquipmentSupportCommitError::EquipmentBusy {
                equipment,
                job,
                release,
            }
        }
        EquipmentSupportBlocker::Mining { job } => {
            EquipmentSupportCommitError::EquipmentBusyMining { equipment, job }
        }
        EquipmentSupportBlocker::ManualPower => {
            EquipmentSupportCommitError::EquipmentBusyManualPower { equipment }
        }
        EquipmentSupportBlocker::Prospecting { completes_at } => {
            EquipmentSupportCommitError::EquipmentBusyProspecting {
                equipment,
                completes_at,
            }
        }
        EquipmentSupportBlocker::TreeFelling { completes_at } => {
            EquipmentSupportCommitError::EquipmentBusyTreeFelling {
                equipment,
                completes_at,
            }
        }
        EquipmentSupportBlocker::BlockExcavation { completes_at } => {
            EquipmentSupportCommitError::EquipmentBusyBlockExcavation {
                equipment,
                completes_at,
            }
        }
        EquipmentSupportBlocker::Maintenance { completes_at } => {
            EquipmentSupportCommitError::EquipmentUnderMaintenance {
                equipment,
                completes_at,
            }
        }
    })
}
