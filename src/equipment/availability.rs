//! Canonical cross-owner runtime occupancy query for equipment instances.

use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::mining::MiningJobId;
use crate::production::{ProductionJobId, ProductionOccupancyRelease};

use super::EquipmentId;

/// The canonical in-flight owner currently reserving one equipment instance.
///
/// Trusted state validation guarantees that one equipment instance cannot be held by more than one
/// canonical operation at the same time. Suspended production remains an occupant because the work
/// in process still owns its provider even when a support-repair path may temporarily relocate it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EquipmentOccupancy {
    Production {
        job: ProductionJobId,
        release: ProductionOccupancyRelease,
    },
    Mining {
        job: MiningJobId,
    },
    ManualPower {
        completes_at: SimulationTick,
    },
    Prospecting {
        completes_at: SimulationTick,
    },
    Maintenance {
        completes_at: SimulationTick,
    },
}

/// Returns the authoritative current owner of one equipment instance, if it is occupied.
pub(crate) fn equipment_occupancy(
    state: &AppState,
    equipment: EquipmentId,
) -> Option<EquipmentOccupancy> {
    if let Some(job) = state.production().get_equipment_occupant(equipment) {
        return Some(EquipmentOccupancy::Production {
            job: job.id(),
            release: job.occupancy_release(),
        });
    }
    if let Some(job) = state.mining().get_equipment_occupant(equipment) {
        return Some(EquipmentOccupancy::Mining { job });
    }
    if let Some(work) = state
        .player_work()
        .get_manual_power_equipment_occupant(equipment)
    {
        return Some(EquipmentOccupancy::ManualPower {
            completes_at: work.completes_at(),
        });
    }
    if let Some(work) = state
        .player_work()
        .get_prospecting_equipment_occupant(equipment)
    {
        return Some(EquipmentOccupancy::Prospecting {
            completes_at: work.completes_at(),
        });
    }
    state
        .player_work()
        .get_equipment_maintenance_occupant(equipment)
        .map(|work| EquipmentOccupancy::Maintenance {
            completes_at: work.completes_at(),
        })
}
