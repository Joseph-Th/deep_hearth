//! Canonical finite-energy store allocation and revision-bound consumption.

use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::core::quantity::{Energy, Power};
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::production::ProductionJobId;
use crate::registry::Registries;

use super::definitions::{EnergyCarrier, EnergyStoreDefinitionId};
use super::state::{EnergyState, EnergyStoreId, EnergyStoreRecord};

/// Failure while allocating an authoritative finite energy store.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddEnergyStoreError {
    UnknownDefinition { definition: EnergyStoreDefinitionId },
    InitialEnergyExceedsCapacity { initial: Energy, capacity: Energy },
    IdExhausted,
    RevisionExhausted,
}

impl Display for AddEnergyStoreError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownDefinition { definition } => write!(
                formatter,
                "unknown energy store definition {}",
                definition.value()
            ),
            Self::InitialEnergyExceedsCapacity { initial, capacity } => write!(
                formatter,
                "initial energy {} nJ exceeds store capacity {} nJ",
                initial.nanojoules(),
                capacity.nanojoules()
            ),
            Self::IdExhausted => formatter.write_str("energy store identifier space is exhausted"),
            Self::RevisionExhausted => {
                formatter.write_str("energy state revision space is exhausted")
            }
        }
    }
}

impl Error for AddEnergyStoreError {}

/// Allocates one empty finite energy store.
///
/// Runtime allocation never creates energy. Charging/generation systems must transfer energy into
/// stores through their own conserved source path when those owners exist.
pub fn add_energy_store(
    registries: &Registries,
    state: &mut AppState,
    definition: EnergyStoreDefinitionId,
) -> Result<EnergyStoreId, AddEnergyStoreError> {
    allocate_energy_store(registries, state, definition, Energy::ZERO)
}

fn allocate_energy_store(
    registries: &Registries,
    state: &mut AppState,
    definition: EnergyStoreDefinitionId,
    initial: Energy,
) -> Result<EnergyStoreId, AddEnergyStoreError> {
    let Some(authored) = registries.energy().get_store(definition) else {
        return Err(AddEnergyStoreError::UnknownDefinition { definition });
    };
    if initial > authored.capacity() {
        return Err(AddEnergyStoreError::InitialEnergyExceedsCapacity {
            initial,
            capacity: authored.capacity(),
        });
    }
    let energy = state.energy_state();
    let id = EnergyStoreId::new(energy.next_store_id);
    let next_store_id = energy
        .next_store_id
        .checked_add(1)
        .ok_or(AddEnergyStoreError::IdExhausted)?;
    let next_revision = energy
        .revision
        .checked_add(1)
        .ok_or(AddEnergyStoreError::RevisionExhausted)?;
    let record = EnergyStoreRecord {
        id,
        definition,
        stored: initial,
        created_at: state.tick(),
    };

    let energy = state.energy_state_mut();
    let previous = energy.records.insert(id, record);
    debug_assert!(
        previous.is_none(),
        "Runtime Invariant 4 (Index Uniqueness): energy store allocation replaced an existing record"
    );
    energy.next_store_id = next_store_id;
    energy.revision = next_revision;
    Ok(id)
}

#[cfg(test)]
pub(crate) fn add_energy_store_with_initial_for_test(
    registries: &Registries,
    state: &mut AppState,
    definition: EnergyStoreDefinitionId,
    initial: Energy,
) -> Result<EnergyStoreId, AddEnergyStoreError> {
    allocate_energy_store(registries, state, definition, initial)
}

/// Exact energy/provenance snapshot moved from a finite store into an operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
    InsufficientEnergy {
        store: EnergyStoreId,
        available: Energy,
        requested: Energy,
    },
    StoreBusy {
        store: EnergyStoreId,
        job: ProductionJobId,
        completes_at: SimulationTick,
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
                completes_at,
            } => write!(
                formatter,
                "energy store {} is reserved by production job {} until tick {}",
                store.value(),
                job.value(),
                completes_at.value()
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
    if let Some(job) = state.production().jobs().find(|job| {
        job.consumed_energy()
            .is_some_and(|trace| trace.source() == store)
    }) {
        return Err(EnergySupplyError::StoreBusy {
            store,
            job: job.id(),
            completes_at: job.completes_at(),
        });
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EnergyConsumptionReservation {
    expected_revision: u64,
    next_revision: u64,
    trace: ConsumedEnergyTrace,
}

impl EnergyConsumptionReservation {
    pub(crate) const fn expected_revision(self) -> u64 {
        self.expected_revision
    }

    pub(crate) const fn trace(self) -> ConsumedEnergyTrace {
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
    if state.revision != selection.expected_revision {
        return Err(EnergyReservationError::StaleSelection {
            expected: selection.expected_revision,
            actual: state.revision,
        });
    }
    let trace = selection.trace;
    let Some(record) = state.records.get(&trace.source) else {
        return Err(EnergyReservationError::UnknownStore {
            store: trace.source,
        });
    };
    if record.stored < trace.energy {
        return Err(EnergyReservationError::InsufficientEnergy {
            store: trace.source,
            available: record.stored,
            requested: trace.energy,
        });
    }
    let next_revision = state
        .revision
        .checked_add(1)
        .ok_or(EnergyReservationError::RevisionExhausted)?;
    Ok(EnergyConsumptionReservation {
        expected_revision: state.revision,
        next_revision,
        trace,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EnergyCommitError {
    StaleRevision { expected: u64, actual: u64 },
}

pub(crate) fn apply_energy_consumption_reservation(
    state: &mut EnergyState,
    reservation: EnergyConsumptionReservation,
) -> Result<ConsumedEnergyTrace, EnergyCommitError> {
    if state.revision != reservation.expected_revision {
        return Err(EnergyCommitError::StaleRevision {
            expected: reservation.expected_revision,
            actual: state.revision,
        });
    }
    let trace = reservation.trace;
    let Some(record) = state.records.get_mut(&trace.source) else {
        debug_assert!(false, "prevalidated energy store disappeared before commit");
        unreachable!("prevalidated energy store disappeared before commit");
    };
    record.stored = match record.stored.checked_sub(trace.energy) {
        Some(stored) => stored,
        None => {
            debug_assert!(
                false,
                "prevalidated energy amount disappeared before commit"
            );
            unreachable!("prevalidated energy amount disappeared before commit");
        }
    };
    state.revision = reservation.next_revision;
    Ok(trace)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::make_test_registries_with_energy_store;
    use crate::core::time::WorldSeed;

    const STORE_DEFINITION: EnergyStoreDefinitionId = EnergyStoreDefinitionId::new(930_001);

    fn registries() -> Registries {
        make_test_registries_with_energy_store(super::super::EnergyStoreDefinition::new(
            STORE_DEFINITION,
            "energy execution fixture",
            EnergyCarrier::Electrical,
            Energy::from_nanojoules(1_000),
            Power::from_microwatts(25),
        ))
    }

    #[test]
    fn allocation_rejects_energy_above_authored_capacity_without_mutation() {
        let registries = registries();
        let mut state = AppState::new(WorldSeed::new(0x9300_0001));
        let before = state.clone();

        assert_eq!(
            add_energy_store_with_initial_for_test(
                &registries,
                &mut state,
                STORE_DEFINITION,
                Energy::from_nanojoules(1_001),
            ),
            Err(AddEnergyStoreError::InitialEnergyExceedsCapacity {
                initial: Energy::from_nanojoules(1_001),
                capacity: Energy::from_nanojoules(1_000),
            })
        );
        assert_eq!(state, before);
    }

    #[test]
    fn runtime_allocation_creates_empty_store_without_free_energy() {
        let registries = registries();
        let mut state = AppState::new(WorldSeed::new(0x9300_0005));

        let store = match add_energy_store(&registries, &mut state, STORE_DEFINITION) {
            Ok(store) => store,
            Err(error) => panic!("runtime energy-store allocation failed: {error}"),
        };

        assert_eq!(
            state
                .energy()
                .get_store(store)
                .map(EnergyStoreRecord::stored),
            Some(Energy::ZERO)
        );
        assert_eq!(state.energy().revision(), 1);
    }

    #[test]
    fn validated_supply_consumes_exact_energy_and_preserves_trace() {
        let registries = registries();
        let mut state = AppState::new(WorldSeed::new(0x9300_0002));
        let store = match add_energy_store_with_initial_for_test(
            &registries,
            &mut state,
            STORE_DEFINITION,
            Energy::from_nanojoules(900),
        ) {
            Ok(store) => store,
            Err(error) => panic!("energy store fixture failed: {error}"),
        };
        let supply = match validate_energy_supply(
            &registries,
            &state,
            store,
            Energy::from_nanojoules(275),
        ) {
            Ok(supply) => supply,
            Err(error) => panic!("energy supply validation failed: {error}"),
        };
        assert_eq!(supply.max_output_power(), Power::from_microwatts(25));
        let reservation =
            match validate_energy_consumption_reservation(state.energy_state(), supply) {
                Ok(reservation) => reservation,
                Err(error) => panic!("energy reservation failed: {error:?}"),
            };
        let trace =
            match apply_energy_consumption_reservation(state.energy_state_mut(), reservation) {
                Ok(trace) => trace,
                Err(error) => panic!("energy consumption commit failed: {error:?}"),
            };

        assert_eq!(trace.source(), store);
        assert_eq!(trace.definition(), STORE_DEFINITION);
        assert_eq!(trace.carrier(), EnergyCarrier::Electrical);
        assert_eq!(trace.energy(), Energy::from_nanojoules(275));
        assert_eq!(
            state
                .energy()
                .get_store(store)
                .map(EnergyStoreRecord::stored),
            Some(Energy::from_nanojoules(625))
        );
        assert_eq!(state.energy().revision(), 2);
    }

    #[test]
    fn stale_supply_is_rejected_after_independent_energy_mutation() {
        let registries = registries();
        let mut state = AppState::new(WorldSeed::new(0x9300_0003));
        let store = match add_energy_store_with_initial_for_test(
            &registries,
            &mut state,
            STORE_DEFINITION,
            Energy::from_nanojoules(900),
        ) {
            Ok(store) => store,
            Err(error) => panic!("energy store fixture failed: {error}"),
        };
        let supply = match validate_energy_supply(
            &registries,
            &state,
            store,
            Energy::from_nanojoules(100),
        ) {
            Ok(supply) => supply,
            Err(error) => panic!("energy supply validation failed: {error}"),
        };
        let expected = state.energy().revision();
        if let Err(error) = add_energy_store(&registries, &mut state, STORE_DEFINITION) {
            panic!("independent energy allocation failed: {error}");
        }
        let before = state.clone();

        assert_eq!(
            validate_energy_consumption_reservation(state.energy_state(), supply),
            Err(EnergyReservationError::StaleSelection {
                expected,
                actual: expected + 1,
            })
        );
        assert_eq!(state, before);
    }

    #[test]
    fn supply_rejects_insufficient_energy_without_mutation() {
        let registries = registries();
        let mut state = AppState::new(WorldSeed::new(0x9300_0004));
        let store = match add_energy_store_with_initial_for_test(
            &registries,
            &mut state,
            STORE_DEFINITION,
            Energy::from_nanojoules(50),
        ) {
            Ok(store) => store,
            Err(error) => panic!("energy store fixture failed: {error}"),
        };
        let before = state.clone();

        assert_eq!(
            validate_energy_supply(&registries, &state, store, Energy::from_nanojoules(51),),
            Err(EnergySupplyError::InsufficientEnergy {
                store,
                available: Energy::from_nanojoules(50),
                requested: Energy::from_nanojoules(51),
            })
        );
        assert_eq!(state, before);
    }
}
