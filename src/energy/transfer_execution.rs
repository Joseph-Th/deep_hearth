//! Atomic same-carrier relocation between finite energy stores after a physical path resolver has authorized the transfer.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Energy;
use crate::core::state::AppState;
use crate::production::{ProductionJobId, ProductionOccupancyRelease};
use crate::registry::Registries;

use super::{EnergyCarrier, EnergyStoreId, EnergyStoreRecord};

/// Immutable output of a future electrical, mechanical, or thermal path resolver.
///
/// There is intentionally no public constructor. Store ownership proves that an already-resolved
/// transfer is still valid and commits it atomically; it does not authorize pathless energy
/// teleportation or carrier conversion. Network topology, conversion losses, and generation remain
/// separate physical-system responsibilities.
#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub struct EnergyTransferResolution {
    source: EnergyStoreId,
    destination: EnergyStoreId,
    energy: Energy,
}

impl EnergyTransferResolution {
    #[must_use]
    pub const fn source(&self) -> EnergyStoreId {
        self.source
    }

    #[must_use]
    pub const fn destination(&self) -> EnergyStoreId {
        self.destination
    }

    #[must_use]
    pub const fn energy(&self) -> Energy {
        self.energy
    }
}

#[cfg(test)]
pub(crate) const fn make_test_energy_transfer_resolution(
    source: EnergyStoreId,
    destination: EnergyStoreId,
    energy: Energy,
) -> EnergyTransferResolution {
    EnergyTransferResolution {
        source,
        destination,
        energy,
    }
}

/// Failure while validating one already physically resolved finite-energy transfer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnergyTransferError {
    SameStore {
        store: EnergyStoreId,
    },
    ZeroEnergy,
    UnknownSource {
        store: EnergyStoreId,
    },
    UnknownDestination {
        store: EnergyStoreId,
    },
    UnknownSourceDefinition {
        store: EnergyStoreId,
    },
    UnknownDestinationDefinition {
        store: EnergyStoreId,
    },
    SourceHasNoOutputPower {
        store: EnergyStoreId,
    },
    DestinationHasNoInputPower {
        store: EnergyStoreId,
    },
    CarrierMismatch {
        source: EnergyCarrier,
        destination: EnergyCarrier,
    },
    SourceBusy {
        store: EnergyStoreId,
        job: ProductionJobId,
        release: ProductionOccupancyRelease,
    },
    SourceBusyManualPower {
        store: EnergyStoreId,
    },
    DestinationBusy {
        store: EnergyStoreId,
        job: ProductionJobId,
        release: ProductionOccupancyRelease,
    },
    DestinationBusyManualPower {
        store: EnergyStoreId,
    },
    InsufficientSourceEnergy {
        store: EnergyStoreId,
        available: Energy,
        requested: Energy,
    },
    DestinationEnergyOverflow {
        store: EnergyStoreId,
    },
    DestinationCapacityExceeded {
        store: EnergyStoreId,
        stored: Energy,
        requested: Energy,
        capacity: Energy,
    },
    RevisionExhausted,
}

impl Display for EnergyTransferError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SameStore { store } => write!(
                formatter,
                "energy store {} cannot transfer into itself",
                store.value()
            ),
            Self::ZeroEnergy => formatter.write_str("energy transfer must be nonzero"),
            Self::UnknownSource { store } => {
                write!(formatter, "unknown source energy store {}", store.value())
            }
            Self::UnknownDestination { store } => write!(
                formatter,
                "unknown destination energy store {}",
                store.value()
            ),
            Self::UnknownSourceDefinition { store } => write!(
                formatter,
                "source energy store {} references an unknown definition",
                store.value()
            ),
            Self::UnknownDestinationDefinition { store } => write!(
                formatter,
                "destination energy store {} references an unknown definition",
                store.value()
            ),
            Self::SourceHasNoOutputPower { store } => write!(
                formatter,
                "source energy store {} has no authored output-power capability",
                store.value()
            ),
            Self::DestinationHasNoInputPower { store } => write!(
                formatter,
                "destination energy store {} has no authored input-power capability",
                store.value()
            ),
            Self::CarrierMismatch {
                source,
                destination,
            } => write!(
                formatter,
                "energy storage transfer cannot implicitly convert {source:?} energy into {destination:?} energy"
            ),
            Self::SourceBusy {
                store,
                job,
                release,
            } => write!(
                formatter,
                "source energy store {} is reserved by production job {} {release}",
                store.value(),
                job.value()
            ),
            Self::SourceBusyManualPower { store } => write!(
                formatter,
                "source energy store {} is reserved by direct player-powered generation",
                store.value()
            ),
            Self::DestinationBusy {
                store,
                job,
                release,
            } => write!(
                formatter,
                "destination energy store {} is reserved by production job {} {release}",
                store.value(),
                job.value()
            ),
            Self::DestinationBusyManualPower { store } => write!(
                formatter,
                "destination energy store {} is reserved by direct player-powered generation",
                store.value()
            ),
            Self::InsufficientSourceEnergy {
                store,
                available,
                requested,
            } => write!(
                formatter,
                "source energy store {} contains {} nJ but transfer requires {} nJ",
                store.value(),
                available.nanojoules(),
                requested.nanojoules()
            ),
            Self::DestinationEnergyOverflow { store } => write!(
                formatter,
                "energy transfer overflows destination store {} accounting",
                store.value()
            ),
            Self::DestinationCapacityExceeded {
                store,
                stored,
                requested,
                capacity,
            } => write!(
                formatter,
                "destination energy store {} contains {} nJ and cannot accept {} nJ within capacity {} nJ",
                store.value(),
                stored.nanojoules(),
                requested.nanojoules(),
                capacity.nanojoules()
            ),
            Self::RevisionExhausted => {
                formatter.write_str("energy state revision space is exhausted")
            }
        }
    }
}

impl Error for EnergyTransferError {}

/// Consumed proof that one resolved energy relocation can commit against exact owner revisions.
#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub struct ValidatedEnergyTransfer {
    expected_energy_revision: u64,
    next_energy_revision: u64,
    expected_production_revision: u64,
    resolution: EnergyTransferResolution,
    carrier: EnergyCarrier,
    source_before: Energy,
    destination_before: Energy,
    source_after: Energy,
    destination_after: Energy,
}

/// Validates store identity, carrier, occupancy, quantity, capacity, and owner revisions without
/// mutating authoritative state.
pub fn validate_energy_transfer(
    registries: &Registries,
    state: &AppState,
    resolution: EnergyTransferResolution,
) -> Result<ValidatedEnergyTransfer, EnergyTransferError> {
    let EnergyTransferResolution {
        source,
        destination,
        energy,
    } = resolution;
    if source == destination {
        return Err(EnergyTransferError::SameStore { store: source });
    }
    if energy.is_zero() {
        return Err(EnergyTransferError::ZeroEnergy);
    }

    let source_record = state
        .energy()
        .get_store(source)
        .ok_or(EnergyTransferError::UnknownSource { store: source })?;
    let destination_record = state
        .energy()
        .get_store(destination)
        .ok_or(EnergyTransferError::UnknownDestination { store: destination })?;
    let source_definition = registries
        .energy()
        .get_store(source_record.definition())
        .ok_or(EnergyTransferError::UnknownSourceDefinition { store: source })?;
    let destination_definition = registries
        .energy()
        .get_store(destination_record.definition())
        .ok_or(EnergyTransferError::UnknownDestinationDefinition { store: destination })?;

    if source_definition.max_output_power().is_zero() {
        return Err(EnergyTransferError::SourceHasNoOutputPower { store: source });
    }
    if destination_definition.max_input_power().is_zero() {
        return Err(EnergyTransferError::DestinationHasNoInputPower { store: destination });
    }
    if source_definition.carrier() != destination_definition.carrier() {
        return Err(EnergyTransferError::CarrierMismatch {
            source: source_definition.carrier(),
            destination: destination_definition.carrier(),
        });
    }
    if let Some((job, release)) = get_energy_store_occupant(state, source) {
        return Err(EnergyTransferError::SourceBusy {
            store: source,
            job,
            release,
        });
    }
    if state
        .player_work()
        .get_manual_power_energy_occupant(source)
        .is_some()
    {
        return Err(EnergyTransferError::SourceBusyManualPower { store: source });
    }
    if let Some((job, release)) = get_energy_store_occupant(state, destination) {
        return Err(EnergyTransferError::DestinationBusy {
            store: destination,
            job,
            release,
        });
    }
    if state
        .player_work()
        .get_manual_power_energy_occupant(destination)
        .is_some()
    {
        return Err(EnergyTransferError::DestinationBusyManualPower { store: destination });
    }
    if source_record.stored() < energy {
        return Err(EnergyTransferError::InsufficientSourceEnergy {
            store: source,
            available: source_record.stored(),
            requested: energy,
        });
    }

    let destination_after = destination_record
        .stored()
        .checked_add(energy)
        .ok_or(EnergyTransferError::DestinationEnergyOverflow { store: destination })?;
    if destination_after > destination_definition.capacity() {
        return Err(EnergyTransferError::DestinationCapacityExceeded {
            store: destination,
            stored: destination_record.stored(),
            requested: energy,
            capacity: destination_definition.capacity(),
        });
    }
    let source_after = source_record.stored().checked_sub(energy).ok_or(
        EnergyTransferError::InsufficientSourceEnergy {
            store: source,
            available: source_record.stored(),
            requested: energy,
        },
    )?;
    let expected_energy_revision = state.energy().revision();
    let next_energy_revision = expected_energy_revision
        .checked_add(1)
        .ok_or(EnergyTransferError::RevisionExhausted)?;

    Ok(ValidatedEnergyTransfer {
        expected_energy_revision,
        next_energy_revision,
        expected_production_revision: state.production().revision(),
        resolution,
        carrier: source_definition.carrier(),
        source_before: source_record.stored(),
        destination_before: destination_record.stored(),
        source_after,
        destination_after,
    })
}

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

/// Failure when a validated energy transfer no longer matches its exact owner snapshots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnergyTransferCommitError {
    StaleEnergyRevision { expected: u64, actual: u64 },
    StaleProductionRevision { expected: u64, actual: u64 },
    SourceBusyManualPower { store: EnergyStoreId },
    DestinationBusyManualPower { store: EnergyStoreId },
    SourceChanged { store: EnergyStoreId },
    DestinationChanged { store: EnergyStoreId },
}

impl Display for EnergyTransferCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleEnergyRevision { expected, actual } => write!(
                formatter,
                "validated energy transfer expected energy revision {expected} but current revision is {actual}"
            ),
            Self::StaleProductionRevision { expected, actual } => write!(
                formatter,
                "validated energy transfer expected production revision {expected} but current revision is {actual}"
            ),
            Self::SourceBusyManualPower { store } => write!(
                formatter,
                "energy transfer source {} became reserved by direct player-powered generation",
                store.value()
            ),
            Self::DestinationBusyManualPower { store } => write!(
                formatter,
                "energy transfer destination {} became reserved by direct player-powered generation",
                store.value()
            ),
            Self::SourceChanged { store } => write!(
                formatter,
                "energy transfer source {} changed without the validated owner revision",
                store.value()
            ),
            Self::DestinationChanged { store } => write!(
                formatter,
                "energy transfer destination {} changed without the validated owner revision",
                store.value()
            ),
        }
    }
}

impl Error for EnergyTransferCommitError {}

/// Observable result of one committed same-carrier finite-energy relocation.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EnergyTransferOutcome {
    source: EnergyStoreId,
    destination: EnergyStoreId,
    carrier: EnergyCarrier,
    energy: Energy,
}

impl EnergyTransferOutcome {
    #[must_use]
    pub const fn source(self) -> EnergyStoreId {
        self.source
    }

    #[must_use]
    pub const fn destination(self) -> EnergyStoreId {
        self.destination
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

impl ValidatedEnergyTransfer {
    /// Commits this transfer exactly once after rechecking both authoritative owner revisions and
    /// the exact source/destination energy snapshots.
    pub fn commit(
        self,
        state: &mut AppState,
    ) -> Result<EnergyTransferOutcome, EnergyTransferCommitError> {
        let Self {
            expected_energy_revision,
            next_energy_revision,
            expected_production_revision,
            resolution,
            carrier,
            source_before,
            destination_before,
            source_after,
            destination_after,
        } = self;
        let EnergyTransferResolution {
            source,
            destination,
            energy,
        } = resolution;

        if state
            .player_work()
            .get_manual_power_energy_occupant(source)
            .is_some()
        {
            return Err(EnergyTransferCommitError::SourceBusyManualPower { store: source });
        }
        if state
            .player_work()
            .get_manual_power_energy_occupant(destination)
            .is_some()
        {
            return Err(EnergyTransferCommitError::DestinationBusyManualPower {
                store: destination,
            });
        }

        let actual_energy_revision = state.energy().revision();
        if actual_energy_revision != expected_energy_revision {
            return Err(EnergyTransferCommitError::StaleEnergyRevision {
                expected: expected_energy_revision,
                actual: actual_energy_revision,
            });
        }
        let actual_production_revision = state.production().revision();
        if actual_production_revision != expected_production_revision {
            return Err(EnergyTransferCommitError::StaleProductionRevision {
                expected: expected_production_revision,
                actual: actual_production_revision,
            });
        }
        if state
            .energy()
            .get_store(source)
            .map(EnergyStoreRecord::stored)
            != Some(source_before)
        {
            return Err(EnergyTransferCommitError::SourceChanged { store: source });
        }
        if state
            .energy()
            .get_store(destination)
            .map(EnergyStoreRecord::stored)
            != Some(destination_before)
        {
            return Err(EnergyTransferCommitError::DestinationChanged { store: destination });
        }

        let energy_state = state.energy_state_mut();
        energy_state.apply_transfer_contents(
            source,
            source_after,
            destination,
            destination_after,
            next_energy_revision,
        );

        Ok(EnergyTransferOutcome {
            source,
            destination,
            carrier,
            energy,
        })
    }
}

#[cfg(test)]
#[path = "transfer_execution_tests.rs"]
mod tests;
