//! Canonical revision-bound finite-energy storage transactions.
//!
//! Supply and sink flows share one occupancy boundary, but keep their physical selection and
//! reservation machinery separate so discharge and deferred-ingress semantics cannot drift into
//! one large mixed transaction owner.

mod sink;
mod supply;

pub use sink::{EnergySinkError, ReleasedEnergyTrace, ValidatedEnergySink, validate_energy_sink};
pub use supply::{
    ConsumedEnergyTrace, EnergySupplyError, ValidatedEnergySupply, validate_energy_supply,
};

pub(crate) use sink::{
    EnergyIngressReservation, EnergyIngressReservationError, EnergySinkCapacityError,
    ValidatedEnergySinkAccess, apply_released_energy_outcomes,
    assert_released_energy_outcomes_available, validate_energy_ingress_reservation,
    validate_energy_sink_access, validate_energy_sink_capacity_at_release,
    validate_energy_sink_release,
};
pub(crate) use supply::{
    EnergyConsumptionReservation, EnergyReservationError,
    apply_prechecked_energy_consumption_reservation, assess_energy_supply_access,
    validate_energy_consumption_reservation,
};

#[cfg(test)]
use sink::project_energy_sink_stored_at_release;
#[cfg(test)]
pub(crate) use supply::apply_energy_consumption_reservation;

#[cfg(test)]
#[path = "storage_execution_tests.rs"]
mod tests;
