//! Diagnostics for finite energy-sink binding and deferred ingress reservation.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Energy;
use crate::energy::{EnergyStoreDefinitionId, EnergyStoreId};
use crate::production::{ProductionJobId, ProductionOccupancyRelease};

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
