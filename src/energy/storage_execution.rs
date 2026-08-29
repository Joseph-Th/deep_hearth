//! Canonical revision-bound finite-energy supply, sink, reservation, and commit transactions.

use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::core::quantity::{Energy, Power};
use crate::core::state::AppState;
use crate::production::{ProductionJobId, ProductionOccupancyRelease};
use crate::registry::Registries;

use super::definitions::{EnergyCarrier, EnergyStoreDefinitionId};
use super::state::{EnergyState, EnergyStoreId};

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

/// Failure while binding a finite energy supply before process resolution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnergySupplyError {
    UnknownStore {
        store: EnergyStoreId,
    },
    UnknownDefinition {
        store: EnergyStoreId,
        definition: EnergyStoreDefinitionId,
    },
    ZeroEnergy,
    NoOutputPower {
        store: EnergyStoreId,
    },
    InsufficientEnergy {
        store: EnergyStoreId,
        available: Energy,
        requested: Energy,
    },
    StoreBusy {
        store: EnergyStoreId,
        job: ProductionJobId,
        release: ProductionOccupancyRelease,
    },
    StoreBusyManualPower {
        store: EnergyStoreId,
    },
}

impl Display for EnergySupplyError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownStore { store } => {
                write!(formatter, "unknown energy store {}", store.value())
            }
            Self::UnknownDefinition { store, definition } => write!(
                formatter,
                "energy store {} references unknown definition {}",
                store.value(),
                definition.value()
            ),
            Self::ZeroEnergy => formatter.write_str("energy supply request must be nonzero"),
            Self::NoOutputPower { store } => write!(
                formatter,
                "energy store {} has no authored output-power capability",
                store.value()
            ),
            Self::InsufficientEnergy {
                store,
                available,
                requested,
            } => write!(
                formatter,
                "energy store {} has {} nJ but operation requires {} nJ",
                store.value(),
                available.nanojoules(),
                requested.nanojoules()
            ),
            Self::StoreBusy {
                store,
                job,
                release,
            } => write!(
                formatter,
                "energy store {} is reserved by production job {} {release}",
                store.value(),
                job.value()
            ),
            Self::StoreBusyManualPower { store } => write!(
                formatter,
                "energy store {} is reserved by direct player-powered generation",
                store.value()
            ),
        }
    }
}

impl Error for EnergySupplyError {}

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

/// Failure while binding exact released energy to a finite sink.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnergySinkError {
    UnknownStore {
        store: EnergyStoreId,
    },
    UnknownDefinition {
        store: EnergyStoreId,
        definition: EnergyStoreDefinitionId,
    },
    ZeroEnergy,
    NoInputPower {
        store: EnergyStoreId,
    },
    StoreBusy {
        store: EnergyStoreId,
        job: ProductionJobId,
        release: ProductionOccupancyRelease,
    },
    StoreBusyManualPower {
        store: EnergyStoreId,
    },
    CapacityOverflow {
        store: EnergyStoreId,
    },
    InsufficientCapacity {
        store: EnergyStoreId,
        stored: Energy,
        requested: Energy,
        capacity: Energy,
    },
}

impl Display for EnergySinkError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownStore { store } => {
                write!(formatter, "unknown energy sink store {}", store.value())
            }
            Self::UnknownDefinition { store, definition } => write!(
                formatter,
                "energy sink store {} references unknown definition {}",
                store.value(),
                definition.value()
            ),
            Self::ZeroEnergy => formatter.write_str("energy sink request must be nonzero"),
            Self::NoInputPower { store } => write!(
                formatter,
                "energy store {} has no authored input-power capability",
                store.value()
            ),
            Self::StoreBusy {
                store,
                job,
                release,
            } => write!(
                formatter,
                "energy store {} is reserved by production job {} {release}",
                store.value(),
                job.value()
            ),
            Self::StoreBusyManualPower { store } => write!(
                formatter,
                "energy store {} is reserved by direct player-powered generation",
                store.value()
            ),
            Self::CapacityOverflow { store } => write!(
                formatter,
                "energy sink store {} capacity accounting overflowed",
                store.value()
            ),
            Self::InsufficientCapacity {
                store,
                stored,
                requested,
                capacity,
            } => write!(
                formatter,
                "energy sink store {} contains {} nJ and cannot accept {} nJ within capacity {} nJ",
                store.value(),
                stored.nanojoules(),
                requested.nanojoules(),
                capacity.nanojoules()
            ),
        }
    }
}

impl Error for EnergySinkError {}

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
    if let Some((job, release)) = get_energy_store_occupant(state, store) {
        return Err(EnergySinkError::StoreBusy {
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
        return Err(EnergySinkError::StoreBusyManualPower { store });
    }
    let after = record
        .stored()
        .checked_add(requested)
        .ok_or(EnergySinkError::CapacityOverflow { store })?;
    if after > definition.capacity() {
        return Err(EnergySinkError::InsufficientCapacity {
            store,
            stored: record.stored(),
            requested,
            capacity: definition.capacity(),
        });
    }
    Ok(ValidatedEnergySink {
        expected_revision: state.energy().revision(),
        trace: ReleasedEnergyTrace {
            destination: store,
            definition: record.definition(),
            carrier: definition.carrier(),
            energy: requested,
        },
        max_input_power: definition.max_input_power(),
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EnergyIngressReservation {
    expected_revision: u64,
    trace: ReleasedEnergyTrace,
}

impl EnergyIngressReservation {
    pub(crate) const fn expected_revision(self) -> u64 {
        self.expected_revision
    }

    pub(crate) const fn trace(self) -> ReleasedEnergyTrace {
        self.trace
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EnergyIngressReservationError {
    StaleSelection {
        expected: u64,
        actual: u64,
    },
    UnknownStore {
        store: EnergyStoreId,
    },
    CapacityOverflow {
        store: EnergyStoreId,
    },
    InsufficientCapacity {
        store: EnergyStoreId,
        stored: Energy,
        requested: Energy,
        capacity: Energy,
    },
}

pub(crate) fn validate_energy_ingress_reservation(
    registries: &Registries,
    state: &EnergyState,
    selection: ValidatedEnergySink,
) -> Result<EnergyIngressReservation, EnergyIngressReservationError> {
    if state.revision() != selection.expected_revision {
        return Err(EnergyIngressReservationError::StaleSelection {
            expected: selection.expected_revision,
            actual: state.revision(),
        });
    }
    let trace = selection.trace;
    let Some(record) = state.get_store(trace.destination) else {
        return Err(EnergyIngressReservationError::UnknownStore {
            store: trace.destination,
        });
    };
    let capacity = match registries.energy().get_store(record.definition()) {
        Some(definition) => definition.capacity(),
        None => {
            unreachable!("validated energy sink definition disappeared from immutable registry")
        }
    };
    let after = record.stored().checked_add(trace.energy).ok_or(
        EnergyIngressReservationError::CapacityOverflow {
            store: trace.destination,
        },
    )?;
    if after > capacity {
        return Err(EnergyIngressReservationError::InsufficientCapacity {
            store: trace.destination,
            stored: record.stored(),
            requested: trace.energy,
            capacity,
        });
    }
    Ok(EnergyIngressReservation {
        expected_revision: state.revision(),
        trace,
    })
}

pub(crate) fn apply_released_energy_outcomes(
    state: &mut EnergyState,
    expected_revision: u64,
    next_revision: u64,
    traces: &[ReleasedEnergyTrace],
) {
    assert_eq!(
        state.revision(),
        expected_revision,
        "released-energy completion requires its planned energy revision"
    );
    for trace in traces {
        state.add_stored_energy(trace.destination, trace.energy);
    }
    state.apply_revision(next_revision);
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EnergyReservationError {
    StaleSelection {
        expected: u64,
        actual: u64,
    },
    UnknownStore {
        store: EnergyStoreId,
    },
    InsufficientEnergy {
        store: EnergyStoreId,
        available: Energy,
        requested: Energy,
    },
    RevisionExhausted,
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EnergyCommitError {
    StaleRevision { expected: u64, actual: u64 },
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
    assert_eq!(
        state.revision(),
        reservation.expected_revision,
        "prechecked energy reservation requires its validated energy revision"
    );
    let trace = reservation.trace;
    state.subtract_stored_energy(trace.source, trace.energy);
    state.apply_revision(reservation.next_revision);
    trace
}

#[cfg(test)]
#[path = "storage_execution_tests.rs"]
mod tests;
