//! Diagnostics for finite energy-supply selection, reservation, and test commit checks.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Energy;
use crate::energy::{EnergyStoreDefinitionId, EnergyStoreId};
use crate::production::{ProductionJobId, ProductionOccupancyRelease};

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

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EnergyCommitError {
    StaleRevision { expected: u64, actual: u64 },
}
