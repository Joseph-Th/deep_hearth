//! Finite-energy discharge selection, reservation, and exact consumption.

use serde::{Deserialize, Serialize};

use crate::core::quantity::{Energy, Power};
use crate::core::state::AppState;
use crate::registry::Registries;

use super::get_energy_store_occupant;
use crate::energy::definitions::{EnergyCarrier, EnergyStoreDefinitionId};
use crate::energy::state::{EnergyState, EnergyStoreId};

mod errors;

#[cfg(test)]
use errors::EnergyCommitError;
pub(crate) use errors::EnergyReservationError;
pub use errors::EnergySupplyError;

/// Exact energy/provenance snapshot moved from a finite store into an operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsumedEnergyTrace {
    source: EnergyStoreId,
    definition: EnergyStoreDefinitionId,
    carrier: EnergyCarrier,
    energy: Energy,
}

impl ConsumedEnergyTrace {
    #[must_use]
    pub const fn source(self) -> EnergyStoreId {
        self.source
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

/// Read-only revision-bound proof that one store can supply an exact energy amount.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValidatedEnergySupply {
    expected_revision: u64,
    trace: ConsumedEnergyTrace,
    max_output_power: Power,
}

impl ValidatedEnergySupply {
    #[must_use]
    pub const fn trace(self) -> ConsumedEnergyTrace {
        self.trace
    }

    #[must_use]
    pub const fn max_output_power(self) -> Power {
        self.max_output_power
    }
}

/// Binds an exact energy amount to the current energy-state revision without mutation.
pub fn validate_energy_supply(
    registries: &Registries,
    state: &AppState,
    store: EnergyStoreId,
    requested: Energy,
) -> Result<ValidatedEnergySupply, EnergySupplyError> {
    if requested.is_zero() {
        return Err(EnergySupplyError::ZeroEnergy);
    }
    let Some(record) = state.energy().get_store(store) else {
        return Err(EnergySupplyError::UnknownStore { store });
    };
    let Some(definition) = registries.energy().get_store(record.definition()) else {
        return Err(EnergySupplyError::UnknownDefinition {
            store,
            definition: record.definition(),
        });
    };
    if definition.max_output_power().is_zero() {
        return Err(EnergySupplyError::NoOutputPower { store });
    }
    if let Some((job, release)) = get_energy_store_occupant(state, store) {
        return Err(EnergySupplyError::StoreBusy {
            store,
            job,
            release,
        });
    }
    if state
        .player_work()
        .get_manual_power_energy_occupant(store)
        .is_some()
    {
        return Err(EnergySupplyError::StoreBusyManualPower { store });
    }
    if record.stored() < requested {
        return Err(EnergySupplyError::InsufficientEnergy {
            store,
            available: record.stored(),
            requested,
        });
    }
    Ok(ValidatedEnergySupply {
        expected_revision: state.energy().revision(),
        trace: ConsumedEnergyTrace {
            source: store,
            definition: record.definition(),
            carrier: definition.carrier(),
            energy: requested,
        },
        max_output_power: definition.max_output_power(),
    })
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct EnergyConsumptionReservation {
    expected_revision: u64,
    next_revision: u64,
    trace: ConsumedEnergyTrace,
}

impl EnergyConsumptionReservation {
    pub(crate) const fn expected_revision(&self) -> u64 {
        self.expected_revision
    }

    pub(crate) const fn trace(&self) -> ConsumedEnergyTrace {
        self.trace
    }

    pub(crate) fn assert_matches_state(&self, state: &EnergyState) {
        assert_eq!(
            state.revision(),
            self.expected_revision,
            "energy consumption reservation requires its validated owner revision"
        );
        assert_eq!(
            self.expected_revision.checked_add(1),
            Some(self.next_revision),
            "energy consumption reservation must advance the energy revision exactly once"
        );
        let record = state.get_store(self.trace.source).unwrap_or_else(|| {
            panic!(
                "validated energy source {} disappeared before commit",
                self.trace.source.value()
            )
        });
        assert_eq!(
            record.definition(),
            self.trace.definition,
            "validated energy source definition changed before commit"
        );
        assert!(
            record.stored() >= self.trace.energy,
            "validated energy source no longer contains reserved energy"
        );
    }
}

pub(crate) fn validate_energy_consumption_reservation(
    state: &EnergyState,
    selection: ValidatedEnergySupply,
) -> Result<EnergyConsumptionReservation, EnergyReservationError> {
    if state.revision() != selection.expected_revision {
        return Err(EnergyReservationError::StaleSelection {
            expected: selection.expected_revision,
            actual: state.revision(),
        });
    }
    let trace = selection.trace;
    let Some(record) = state.get_store(trace.source) else {
        return Err(EnergyReservationError::UnknownStore {
            store: trace.source,
        });
    };
    if record.stored() < trace.energy {
        return Err(EnergyReservationError::InsufficientEnergy {
            store: trace.source,
            available: record.stored(),
            requested: trace.energy,
        });
    }
    let next_revision = state
        .revision()
        .checked_add(1)
        .ok_or(EnergyReservationError::RevisionExhausted)?;
    Ok(EnergyConsumptionReservation {
        expected_revision: state.revision(),
        next_revision,
        trace,
    })
}

#[cfg(test)]
pub(crate) fn apply_energy_consumption_reservation(
    state: &mut EnergyState,
    reservation: EnergyConsumptionReservation,
) -> Result<ConsumedEnergyTrace, EnergyCommitError> {
    if state.revision() != reservation.expected_revision {
        return Err(EnergyCommitError::StaleRevision {
            expected: reservation.expected_revision,
            actual: state.revision(),
        });
    }
    Ok(apply_prechecked_energy_consumption_reservation(
        state,
        reservation,
    ))
}

/// Applies a finite-energy reservation after a surrounding transaction has checked its revision.
///
/// Cross-owner commits use this infallible form only after all recoverable conflicts have been
/// rejected, so a prior structural mutation cannot be followed by a stale-energy error.
pub(crate) fn apply_prechecked_energy_consumption_reservation(
    state: &mut EnergyState,
    reservation: EnergyConsumptionReservation,
) -> ConsumedEnergyTrace {
    reservation.assert_matches_state(state);
    let trace = reservation.trace;
    state.subtract_stored_energy(trace.source, trace.energy);
    state.apply_revision(reservation.next_revision);
    trace
}
