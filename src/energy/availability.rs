//! Canonical cross-owner runtime occupancy query for finite energy stores.

use crate::core::state::AppState;
use crate::production::{ProductionJobId, ProductionOccupancyRelease};

use super::EnergyStoreId;

/// The canonical in-flight owner currently reserving one finite energy store.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EnergyStoreOccupancy {
    Production {
        job: ProductionJobId,
        release: ProductionOccupancyRelease,
    },
    ManualPower,
}

/// Returns the authoritative current owner of one finite energy store, if it is occupied.
pub(crate) fn energy_store_occupancy(
    state: &AppState,
    store: EnergyStoreId,
) -> Option<EnergyStoreOccupancy> {
    if let Some(job) = state.production().get_energy_occupant(store) {
        let record = state.production().get_job(job).unwrap_or_else(|| {
            panic!(
                "runtime invariant broken: energy occupancy references missing production job {}",
                job.value()
            )
        });
        return Some(EnergyStoreOccupancy::Production {
            job,
            release: record.occupancy_release(),
        });
    }
    state
        .player_work()
        .get_manual_power_energy_occupant(store)
        .is_some()
        .then_some(EnergyStoreOccupancy::ManualPower)
}
