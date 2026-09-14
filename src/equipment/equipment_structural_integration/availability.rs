//! Runtime occupancy policy for equipment support relocation.

use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::equipment::{EquipmentId, EquipmentOccupancy, equipment_occupancy};
use crate::mining::MiningJobId;
use crate::production::{ProductionJobId, ProductionOccupancyRelease};

use super::{EquipmentSupportCommitError, EquipmentSupportError};

/// Current runtime ownership that prevents physically changing an equipment support assignment.
///
/// Suspended production deliberately does not block relocation because support recovery is one of
/// the canonical ways to make that retained work resumable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EquipmentSupportBlocker {
    Production {
        job: ProductionJobId,
        completes_at: SimulationTick,
    },
    Mining {
        job: MiningJobId,
    },
    ManualPower,
    Prospecting {
        completes_at: SimulationTick,
    },
    Maintenance {
        completes_at: SimulationTick,
    },
}

fn blocker(state: &AppState, equipment: EquipmentId) -> Option<EquipmentSupportBlocker> {
    equipment_occupancy(state, equipment).and_then(|occupancy| match occupancy {
        EquipmentOccupancy::Production {
            job,
            release: ProductionOccupancyRelease::Scheduled(completes_at),
        } => Some(EquipmentSupportBlocker::Production { job, completes_at }),
        EquipmentOccupancy::Production {
            release: ProductionOccupancyRelease::AwaitingResume,
            ..
        } => None,
        EquipmentOccupancy::Mining { job } => Some(EquipmentSupportBlocker::Mining { job }),
        EquipmentOccupancy::ManualPower { .. } => Some(EquipmentSupportBlocker::ManualPower),
        EquipmentOccupancy::Prospecting { completes_at } => {
            Some(EquipmentSupportBlocker::Prospecting { completes_at })
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
        EquipmentSupportBlocker::Production { job, completes_at } => {
            EquipmentSupportError::EquipmentBusy {
                equipment,
                job,
                completes_at,
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
        EquipmentSupportBlocker::Production { job, completes_at } => {
            EquipmentSupportCommitError::EquipmentBusy {
                equipment,
                job,
                completes_at,
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
        EquipmentSupportBlocker::Maintenance { completes_at } => {
            EquipmentSupportCommitError::EquipmentUnderMaintenance {
                equipment,
                completes_at,
            }
        }
    })
}
