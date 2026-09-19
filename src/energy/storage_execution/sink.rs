//! Finite-energy ingress selection, deferred-capacity projection, reservation, and completion.

use serde::{Deserialize, Serialize};

use crate::core::quantity::{Energy, Power};
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::energy::{EnergyStoreOccupancy, energy_store_occupancy};
use crate::registry::Registries;

use crate::energy::definitions::{EnergyCarrier, EnergyStoreDefinitionId};
use crate::energy::state::EnergyStoreId;

mod capacity;
mod completion;
mod errors;
mod reservation;

#[cfg(test)]
pub(crate) use capacity::project_energy_sink_stored_at_release;
pub(crate) use capacity::{
    EnergySinkCapacityError, available_energy_sink_capacity_at_release,
    validate_energy_sink_capacity_at_release,
};
pub(crate) use completion::{
    apply_released_energy_outcomes, assert_released_energy_outcomes_available,
};
pub(crate) use errors::EnergyIngressReservationError;
pub use errors::EnergySinkError;
pub(crate) use reservation::{EnergyIngressReservation, validate_energy_ingress_reservation};

/// Revision-bound access to one available finite sink before a deferred release amount is known to
/// fit. Thermal resolution uses this to discover the sink power limit before duration determines
/// how much passive dissipation is guaranteed before completion.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ValidatedEnergySinkAccess {
    expected_revision: u64,
    store: EnergyStoreId,
    definition: EnergyStoreDefinitionId,
    carrier: EnergyCarrier,
    stored: Energy,
    max_input_power: Power,
}

impl ValidatedEnergySinkAccess {
    pub(crate) const fn carrier(self) -> EnergyCarrier {
        self.carrier
    }

    pub(crate) const fn max_input_power(self) -> Power {
        self.max_input_power
    }

    /// Exact capacity guaranteed to remain free when a deferred release becomes authoritative.
    pub(crate) fn available_capacity_at_release(
        self,
        registries: &Registries,
        release_after: TickSpan,
    ) -> Energy {
        available_energy_sink_capacity_at_release(
            registries,
            self.definition,
            self.stored,
            release_after,
        )
    }
}

/// Exact energy released by an in-flight operation and committed to one finite sink at completion.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleasedEnergyTrace {
    destination: EnergyStoreId,
    definition: EnergyStoreDefinitionId,
    carrier: EnergyCarrier,
    energy: Energy,
}

impl ReleasedEnergyTrace {
    #[must_use]
    pub const fn destination(self) -> EnergyStoreId {
        self.destination
    }

    #[must_use]
    pub const fn definition(self) -> EnergyStoreDefinitionId {
        self.definition
    }

    #[must_use]
    pub const fn carrier(self) -> EnergyCarrier {
        self.carrier
    }

    #[must_use]
    pub const fn energy(self) -> Energy {
        self.energy
    }
}

/// Read-only revision-bound proof that one finite store can accept exact released energy.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValidatedEnergySink {
    expected_revision: u64,
    trace: ReleasedEnergyTrace,
    max_input_power: Power,
}

impl ValidatedEnergySink {
    #[must_use]
    pub const fn trace(self) -> ReleasedEnergyTrace {
        self.trace
    }

    #[must_use]
    pub const fn max_input_power(self) -> Power {
        self.max_input_power
    }
}

/// Binds exact released energy to current sink capacity without mutation.
pub fn validate_energy_sink(
    registries: &Registries,
    state: &AppState,
    store: EnergyStoreId,
    requested: Energy,
) -> Result<ValidatedEnergySink, EnergySinkError> {
    if requested.is_zero() {
        return Err(EnergySinkError::ZeroEnergy);
    }
    let access = validate_energy_sink_access(registries, state, store)?;
    validate_energy_sink_release(registries, access, requested, TickSpan::new(0))
}

pub(crate) fn validate_energy_sink_access(
    registries: &Registries,
    state: &AppState,
    store: EnergyStoreId,
) -> Result<ValidatedEnergySinkAccess, EnergySinkError> {
    let Some(record) = state.energy().get_store(store) else {
        return Err(EnergySinkError::UnknownStore { store });
    };
    let Some(definition) = registries.energy().get_store(record.definition()) else {
        return Err(EnergySinkError::UnknownDefinition {
            store,
            definition: record.definition(),
        });
    };
    if definition.max_input_power().is_zero() {
        return Err(EnergySinkError::NoInputPower { store });
    }
    match energy_store_occupancy(state, store) {
        Some(EnergyStoreOccupancy::Production { job, release }) => {
            return Err(EnergySinkError::StoreBusy {
                store,
                job,
                release,
            });
        }
        Some(EnergyStoreOccupancy::ManualPower) => {
            return Err(EnergySinkError::StoreBusyManualPower { store });
        }
        None => {}
    }
    Ok(ValidatedEnergySinkAccess {
        expected_revision: state.energy().revision(),
        store,
        definition: record.definition(),
        carrier: definition.carrier(),
        stored: record.stored(),
        max_input_power: definition.max_input_power(),
    })
}

pub(crate) fn validate_energy_sink_release(
    registries: &Registries,
    access: ValidatedEnergySinkAccess,
    requested: Energy,
    release_after: TickSpan,
) -> Result<ValidatedEnergySink, EnergySinkError> {
    if requested.is_zero() {
        return Err(EnergySinkError::ZeroEnergy);
    }
    validate_energy_sink_capacity_at_release(
        registries,
        access.definition,
        access.stored,
        requested,
        release_after,
    )
    .map_err(|error| match error {
        EnergySinkCapacityError::Overflow => EnergySinkError::CapacityOverflow {
            store: access.store,
        },
        EnergySinkCapacityError::Insufficient {
            stored,
            requested,
            capacity,
        } => EnergySinkError::InsufficientCapacity {
            store: access.store,
            stored,
            requested,
            capacity,
        },
    })?;
    Ok(ValidatedEnergySink {
        expected_revision: access.expected_revision,
        trace: ReleasedEnergyTrace {
            destination: access.store,
            definition: access.definition,
            carrier: access.carrier,
            energy: requested,
        },
        max_input_power: access.max_input_power,
    })
}
