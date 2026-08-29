//! Finite-energy ingress selection, deferred-capacity projection, reservation, and completion.

use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::core::quantity::{Energy, Power};
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::production::{ProductionJobId, ProductionOccupancyRelease};
use crate::registry::Registries;

use super::get_energy_store_occupant;
use crate::energy::definitions::{EnergyCarrier, EnergyStoreDefinitionId};
use crate::energy::passive_dissipation::project_stored_energy_after_passive_dissipation;
use crate::energy::state::{EnergyState, EnergyStoreId};

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
    capacity: Energy,
    max_input_power: Power,
}

impl ValidatedEnergySinkAccess {
    pub(crate) const fn carrier(self) -> EnergyCarrier {
        self.carrier
    }

    pub(crate) const fn max_input_power(self) -> Power {
        self.max_input_power
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
    Ok(ValidatedEnergySinkAccess {
        expected_revision: state.energy().revision(),
        store,
        definition: record.definition(),
        carrier: definition.carrier(),
        stored: record.stored(),
        capacity: definition.capacity(),
        max_input_power: definition.max_input_power(),
    })
}

/// Projects sink contents immediately before a deferred completion releases energy. Completion is
/// applied before passive loss on its due tick, so only the preceding `release_after - 1` ticks can
/// be credited as guaranteed recovery. This is conservative across suspension because extra wall
/// time can only create additional passive capacity.
pub(crate) fn project_energy_sink_stored_at_release(
    registries: &Registries,
    definition: EnergyStoreDefinitionId,
    stored: Energy,
    release_after: TickSpan,
) -> Energy {
    let definition = registries
        .energy()
        .get_store(definition)
        .unwrap_or_else(|| {
            panic!(
                "validated deferred energy sink references missing immutable definition {}",
                definition.value()
            )
        });
    let passive_ticks = TickSpan::new(release_after.value().saturating_sub(1));
    project_stored_energy_after_passive_dissipation(registries, definition, stored, passive_ticks)
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
    let projected_stored = project_energy_sink_stored_at_release(
        registries,
        access.definition,
        access.stored,
        release_after,
    );
    let after =
        projected_stored
            .checked_add(requested)
            .ok_or(EnergySinkError::CapacityOverflow {
                store: access.store,
            })?;
    if after > access.capacity {
        return Err(EnergySinkError::InsufficientCapacity {
            store: access.store,
            stored: projected_stored,
            requested,
            capacity: access.capacity,
        });
    }
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
    release_after: TickSpan,
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
    let projected_stored = project_energy_sink_stored_at_release(
        registries,
        record.definition(),
        record.stored(),
        release_after,
    );
    let after = projected_stored.checked_add(trace.energy).ok_or(
        EnergyIngressReservationError::CapacityOverflow {
            store: trace.destination,
        },
    )?;
    if after > capacity {
        return Err(EnergyIngressReservationError::InsufficientCapacity {
            store: trace.destination,
            stored: projected_stored,
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
