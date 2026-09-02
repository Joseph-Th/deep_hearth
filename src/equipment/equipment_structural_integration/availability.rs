//! Runtime occupancy policy for equipment support relocation.

use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::equipment::EquipmentId;
use crate::mining::MiningJobId;
use crate::production::ProductionJobId;

use super::{EquipmentSupportCommitError, EquipmentSupportError};

/// Current runtime ownership that prevents physically moving an equipment record.
///
/// Production only blocks support changes while running. Suspended production deliberately keeps
/// the equipment reservation while allowing relocation so failed support can be recovered without
/// abandoning work in process.
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
    if let Some(job) = state
        .production()
        .get_equipment_occupant(equipment)
        .filter(|job| !job.is_suspended())
    {
        return Some(EquipmentSupportBlocker::Production {
            job: job.id(),
            completes_at: job.completes_at(),
        });
    }
    if let Some(job) = state.mining().get_equipment_occupant(equipment) {
        return Some(EquipmentSupportBlocker::Mining { job });
    }
    if state
        .player_work()
        .get_manual_power_equipment_occupant(equipment)
        .is_some()
    {
        return Some(EquipmentSupportBlocker::ManualPower);
    }
    if let Some(work) = state
        .player_work()
        .get_prospecting_equipment_occupant(equipment)
    {
        return Some(EquipmentSupportBlocker::Prospecting {
            completes_at: work.completes_at(),
        });
    }
    state
        .player_work()
        .get_equipment_maintenance_occupant(equipment)
        .map(|work| EquipmentSupportBlocker::Maintenance {
            completes_at: work.completes_at(),
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
