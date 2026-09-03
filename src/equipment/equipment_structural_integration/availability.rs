//! Runtime occupancy policy for equipment support relocation.

use crate::core::state::AppState;
use crate::equipment::{EquipmentId, EquipmentOccupancy, equipment_occupancy};
use crate::production::ProductionOccupancyRelease;

use super::{EquipmentSupportCommitError, EquipmentSupportError};

pub(super) fn support_validation_error(
    state: &AppState,
    equipment: EquipmentId,
) -> Option<EquipmentSupportError> {
    equipment_occupancy(state, equipment).and_then(|occupancy| match occupancy {
        EquipmentOccupancy::Production {
            job,
            release: ProductionOccupancyRelease::Scheduled(completes_at),
        } => EquipmentSupportError::EquipmentBusy {
            equipment,
            job,
            completes_at,
        }
        .into(),
        EquipmentOccupancy::Production {
            release: ProductionOccupancyRelease::AwaitingResume,
            ..
        } => None,
        EquipmentOccupancy::Mining { job } => {
            Some(EquipmentSupportError::EquipmentBusyMining { equipment, job })
        }
        EquipmentOccupancy::ManualPower { .. } => {
            Some(EquipmentSupportError::EquipmentBusyManualPower { equipment })
        }
        EquipmentOccupancy::Prospecting { completes_at } => {
            Some(EquipmentSupportError::EquipmentBusyProspecting {
                equipment,
                completes_at,
            })
        }
        EquipmentOccupancy::Maintenance { completes_at } => {
            Some(EquipmentSupportError::EquipmentUnderMaintenance {
                equipment,
                completes_at,
            })
        }
    })
}

pub(super) fn support_commit_error(
    state: &AppState,
    equipment: EquipmentId,
) -> Option<EquipmentSupportCommitError> {
    equipment_occupancy(state, equipment).and_then(|occupancy| match occupancy {
        EquipmentOccupancy::Production {
            job,
            release: ProductionOccupancyRelease::Scheduled(completes_at),
        } => EquipmentSupportCommitError::EquipmentBusy {
            equipment,
            job,
            completes_at,
        }
        .into(),
        EquipmentOccupancy::Production {
            release: ProductionOccupancyRelease::AwaitingResume,
            ..
        } => None,
        EquipmentOccupancy::Mining { job } => {
            Some(EquipmentSupportCommitError::EquipmentBusyMining { equipment, job })
        }
        EquipmentOccupancy::ManualPower { .. } => {
            Some(EquipmentSupportCommitError::EquipmentBusyManualPower { equipment })
        }
        EquipmentOccupancy::Prospecting { completes_at } => {
            Some(EquipmentSupportCommitError::EquipmentBusyProspecting {
                equipment,
                completes_at,
            })
        }
        EquipmentOccupancy::Maintenance { completes_at } => {
            Some(EquipmentSupportCommitError::EquipmentUnderMaintenance {
                equipment,
                completes_at,
            })
        }
    })
}
