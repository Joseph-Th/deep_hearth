//! Canonical revision-bound finite-energy storage transactions.
//!
//! Supply and sink flows share one occupancy boundary, but keep their physical selection and
//! reservation machinery separate so discharge and deferred-ingress semantics cannot drift into
//! one large mixed transaction owner.

use crate::core::state::AppState;
use crate::production::{ProductionJobId, ProductionOccupancyRelease};

use super::state::EnergyStoreId;

mod sink;
mod supply;

pub use sink::{EnergySinkError, ReleasedEnergyTrace, ValidatedEnergySink, validate_energy_sink};
pub use supply::{
    ConsumedEnergyTrace, EnergySupplyError, ValidatedEnergySupply, validate_energy_supply,
};

pub(crate) use sink::{
    EnergyIngressReservation, EnergyIngressReservationError, apply_released_energy_outcomes,
    project_energy_sink_stored_at_release, validate_energy_ingress_reservation,
    validate_energy_sink_access, validate_energy_sink_release,
};
pub(crate) use supply::{
    EnergyConsumptionReservation, EnergyReservationError,
    apply_prechecked_energy_consumption_reservation, validate_energy_consumption_reservation,
};

#[cfg(test)]
pub(crate) use supply::apply_energy_consumption_reservation;

fn get_energy_store_occupant(
    state: &AppState,
    store: EnergyStoreId,
) -> Option<(ProductionJobId, ProductionOccupancyRelease)> {
    let job_id = state.production().get_energy_occupant(store)?;
    let job = match state.production().get_job(job_id) {
        Some(job) => job,
        None => panic!(
            "runtime invariant broken: energy occupancy index references missing production job {}",
            job_id.value()
        ),
    };
    Some((job_id, job.occupancy_release()))
}

#[cfg(test)]
#[path = "storage_execution_tests.rs"]
mod tests;
