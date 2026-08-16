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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EnergyTransferResolution {
    source: EnergyStoreId,
    destination: EnergyStoreId,
    energy: Energy,
}

impl EnergyTransferResolution {
    #[must_use]
    pub const fn source(self) -> EnergyStoreId {
        self.source
    }

    #[must_use]
    pub const fn destination(self) -> EnergyStoreId {
        self.destination
    }

    #[must_use]
    pub const fn energy(self) -> Energy {
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
    DestinationBusy {
        store: EnergyStoreId,
        job: ProductionJobId,
        release: ProductionOccupancyRelease,
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    if let Some((job, release)) = get_energy_store_occupant(state, destination) {
        return Err(EnergyTransferError::DestinationBusy {
            store: destination,
            job,
            release,
        });
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
mod tests {
    use super::*;
    use crate::content::{
        FORM_LOG, MATERIAL_WOOD, make_test_registries_with_energy_stores,
        make_test_registries_with_energy_stores_and_process,
    };
    use crate::core::quantity::{Mass, Power, Temperature};
    use crate::core::state::validate_loaded_state;
    use crate::core::time::WorldSeed;
    use crate::energy::{
        EnergyStoreDefinition, EnergyStoreDefinitionId, add_energy_store,
        add_energy_store_with_initial_for_test,
    };
    use crate::inventory::{add_solid_stockpile_for_test, deposit_bulk_for_test};
    use crate::material::{CommodityKey, MaterialInputSpec, MaterialLotSpec};
    use crate::production::{
        ProcessDefinition, ProcessId, make_test_process_resolution, validate_process_inputs,
        validate_start_process,
    };

    const ELECTRICAL_STORE: EnergyStoreDefinitionId = EnergyStoreDefinitionId::new(931_001);
    const THERMAL_STORE: EnergyStoreDefinitionId = EnergyStoreDefinitionId::new(931_002);
    const TEST_PROCESS: ProcessId = ProcessId::new(931_001);

    fn bidirectional_definition(
        id: EnergyStoreDefinitionId,
        carrier: EnergyCarrier,
    ) -> EnergyStoreDefinition {
        EnergyStoreDefinition::new_with_transfer_limits(
            id,
            "energy transfer fixture",
            carrier,
            Energy::from_nanojoules(1_000),
            Power::from_microwatts(20),
            Power::from_microwatts(25),
        )
    }

    fn stored_energy_total(state: &AppState) -> Energy {
        match state
            .energy()
            .stores()
            .try_fold(Energy::ZERO, |total, store| {
                total.checked_add(store.stored())
            }) {
            Some(total) => total,
            None => panic!("energy transfer fixture total overflowed authoritative accounting"),
        }
    }

    fn run_transfer_soak(seed: WorldSeed) -> AppState {
        let registries = registries();
        let mut state = AppState::new(seed);
        let left = match add_energy_store_with_initial_for_test(
            &registries,
            &mut state,
            ELECTRICAL_STORE,
            Energy::from_nanojoules(600),
        ) {
            Ok(store) => store,
            Err(error) => panic!("energy soak left-store fixture failed: {error}"),
        };
        let right = match add_energy_store_with_initial_for_test(
            &registries,
            &mut state,
            ELECTRICAL_STORE,
            Energy::from_nanojoules(400),
        ) {
            Ok(store) => store,
            Err(error) => panic!("energy soak right-store fixture failed: {error}"),
        };
        let initial_total = stored_energy_total(&state);

        for step in 0..2_000_u64 {
            let (source, destination) = if step.is_multiple_of(2) {
                (left, right)
            } else {
                (right, left)
            };
            let resolution = make_test_energy_transfer_resolution(
                source,
                destination,
                Energy::from_nanojoules(1),
            );
            let validated = match validate_energy_transfer(&registries, &state, resolution) {
                Ok(validated) => validated,
                Err(error) => panic!("energy soak validation failed at step {step}: {error}"),
            };
            if let Err(error) = validated.commit(&mut state) {
                panic!("energy soak commit failed at step {step}: {error}");
            }

            if step.is_multiple_of(137) {
                assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
                assert_eq!(stored_energy_total(&state), initial_total);
            }
        }

        assert_eq!(stored_energy_total(&state), initial_total);
        assert_eq!(
            state
                .energy()
                .get_store(left)
                .map(EnergyStoreRecord::stored),
            Some(Energy::from_nanojoules(600))
        );
        assert_eq!(
            state
                .energy()
                .get_store(right)
                .map(EnergyStoreRecord::stored),
            Some(Energy::from_nanojoules(400))
        );
        state
    }

    fn registries() -> Registries {
        make_test_registries_with_energy_stores(vec![bidirectional_definition(
            ELECTRICAL_STORE,
            EnergyCarrier::Electrical,
        )])
    }

    fn no_energy_process() -> ProcessDefinition {
        ProcessDefinition::new(
            TEST_PROCESS,
            "energy transfer production-revision fixture",
            vec![MaterialInputSpec::new(
                CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
                Mass::from_milligrams(1),
            )],
            Vec::new(),
        )
    }

    #[test]
    fn validated_transfer_conserves_energy_and_advances_revision_once() {
        let registries = registries();
        let mut state = AppState::new(WorldSeed::new(0x9310_0001));
        let source = match add_energy_store_with_initial_for_test(
            &registries,
            &mut state,
            ELECTRICAL_STORE,
            Energy::from_nanojoules(700),
        ) {
            Ok(store) => store,
            Err(error) => panic!("source energy fixture failed: {error}"),
        };
        let destination = match add_energy_store_with_initial_for_test(
            &registries,
            &mut state,
            ELECTRICAL_STORE,
            Energy::from_nanojoules(100),
        ) {
            Ok(store) => store,
            Err(error) => panic!("destination energy fixture failed: {error}"),
        };
        let revision_before = state.energy().revision();
        let resolution =
            make_test_energy_transfer_resolution(source, destination, Energy::from_nanojoules(250));
        let validated = match validate_energy_transfer(&registries, &state, resolution) {
            Ok(validated) => validated,
            Err(error) => panic!("energy transfer validation failed: {error}"),
        };
        let outcome = match validated.commit(&mut state) {
            Ok(outcome) => outcome,
            Err(error) => panic!("energy transfer commit failed: {error}"),
        };

        assert_eq!(outcome.source(), source);
        assert_eq!(outcome.destination(), destination);
        assert_eq!(outcome.carrier(), EnergyCarrier::Electrical);
        assert_eq!(outcome.energy(), Energy::from_nanojoules(250));
        assert_eq!(
            state
                .energy()
                .get_store(source)
                .map(EnergyStoreRecord::stored),
            Some(Energy::from_nanojoules(450))
        );
        assert_eq!(
            state
                .energy()
                .get_store(destination)
                .map(EnergyStoreRecord::stored),
            Some(Energy::from_nanojoules(350))
        );
        let total = state
            .energy()
            .stores()
            .try_fold(Energy::ZERO, |total, store| {
                total.checked_add(store.stored())
            });
        assert_eq!(total, Some(Energy::from_nanojoules(800)));
        assert_eq!(state.energy().revision(), revision_before + 1);
    }

    #[test]
    fn storage_boundary_rejects_implicit_carrier_conversion_without_mutation() {
        let registries = make_test_registries_with_energy_stores(vec![
            bidirectional_definition(ELECTRICAL_STORE, EnergyCarrier::Electrical),
            bidirectional_definition(THERMAL_STORE, EnergyCarrier::Thermal),
        ]);
        let mut state = AppState::new(WorldSeed::new(0x9310_0002));
        let source = match add_energy_store_with_initial_for_test(
            &registries,
            &mut state,
            ELECTRICAL_STORE,
            Energy::from_nanojoules(500),
        ) {
            Ok(store) => store,
            Err(error) => panic!("electrical source fixture failed: {error}"),
        };
        let destination = match add_energy_store(&registries, &mut state, THERMAL_STORE) {
            Ok(store) => store,
            Err(error) => panic!("thermal destination fixture failed: {error}"),
        };
        let before = state.clone();

        assert_eq!(
            validate_energy_transfer(
                &registries,
                &state,
                make_test_energy_transfer_resolution(
                    source,
                    destination,
                    Energy::from_nanojoules(1),
                ),
            ),
            Err(EnergyTransferError::CarrierMismatch {
                source: EnergyCarrier::Electrical,
                destination: EnergyCarrier::Thermal,
            })
        );
        assert_eq!(state, before);
    }

    #[test]
    fn transfer_capacity_failure_is_atomic() {
        let registries = registries();
        let mut state = AppState::new(WorldSeed::new(0x9310_0003));
        let source = match add_energy_store_with_initial_for_test(
            &registries,
            &mut state,
            ELECTRICAL_STORE,
            Energy::from_nanojoules(500),
        ) {
            Ok(store) => store,
            Err(error) => panic!("capacity source fixture failed: {error}"),
        };
        let destination = match add_energy_store_with_initial_for_test(
            &registries,
            &mut state,
            ELECTRICAL_STORE,
            Energy::from_nanojoules(900),
        ) {
            Ok(store) => store,
            Err(error) => panic!("capacity destination fixture failed: {error}"),
        };
        let before = state.clone();

        assert_eq!(
            validate_energy_transfer(
                &registries,
                &state,
                make_test_energy_transfer_resolution(
                    source,
                    destination,
                    Energy::from_nanojoules(101),
                ),
            ),
            Err(EnergyTransferError::DestinationCapacityExceeded {
                store: destination,
                stored: Energy::from_nanojoules(900),
                requested: Energy::from_nanojoules(101),
                capacity: Energy::from_nanojoules(1_000),
            })
        );
        assert_eq!(state, before);
    }

    #[test]
    fn stale_energy_revision_rejects_validated_transfer_without_partial_mutation() {
        let registries = registries();
        let mut state = AppState::new(WorldSeed::new(0x9310_0004));
        let source = match add_energy_store_with_initial_for_test(
            &registries,
            &mut state,
            ELECTRICAL_STORE,
            Energy::from_nanojoules(500),
        ) {
            Ok(store) => store,
            Err(error) => panic!("stale source fixture failed: {error}"),
        };
        let destination = match add_energy_store(&registries, &mut state, ELECTRICAL_STORE) {
            Ok(store) => store,
            Err(error) => panic!("stale destination fixture failed: {error}"),
        };
        let validated = match validate_energy_transfer(
            &registries,
            &state,
            make_test_energy_transfer_resolution(source, destination, Energy::from_nanojoules(100)),
        ) {
            Ok(validated) => validated,
            Err(error) => panic!("stale transfer validation failed: {error}"),
        };
        let expected = state.energy().revision();
        if let Err(error) = add_energy_store(&registries, &mut state, ELECTRICAL_STORE) {
            panic!("independent energy mutation failed: {error}");
        }
        let before_commit = state.clone();

        assert_eq!(
            validated.commit(&mut state),
            Err(EnergyTransferCommitError::StaleEnergyRevision {
                expected,
                actual: expected + 1,
            })
        );
        assert_eq!(state, before_commit);
    }

    #[test]
    fn stale_production_revision_rejects_validated_transfer_without_partial_mutation() {
        let registries = make_test_registries_with_energy_stores_and_process(
            vec![bidirectional_definition(
                ELECTRICAL_STORE,
                EnergyCarrier::Electrical,
            )],
            no_energy_process(),
        );
        let mut state = AppState::new(WorldSeed::new(0x9310_0005));
        let source = match add_energy_store_with_initial_for_test(
            &registries,
            &mut state,
            ELECTRICAL_STORE,
            Energy::from_nanojoules(500),
        ) {
            Ok(store) => store,
            Err(error) => panic!("production-stale source fixture failed: {error}"),
        };
        let destination = match add_energy_store(&registries, &mut state, ELECTRICAL_STORE) {
            Ok(store) => store,
            Err(error) => panic!("production-stale destination fixture failed: {error}"),
        };
        let validated = match validate_energy_transfer(
            &registries,
            &state,
            make_test_energy_transfer_resolution(source, destination, Energy::from_nanojoules(100)),
        ) {
            Ok(validated) => validated,
            Err(error) => panic!("production-stale transfer validation failed: {error}"),
        };
        let expected_energy_revision = state.energy().revision();
        let expected_production_revision = state.production().revision();

        let material_source =
            match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2)) {
                Ok(stockpile) => stockpile,
                Err(error) => panic!("production-stale material source failed: {error}"),
            };
        let material_destination =
            match add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2)) {
                Ok(stockpile) => stockpile,
                Err(error) => panic!("production-stale material destination failed: {error}"),
            };
        if let Err(error) = deposit_bulk_for_test(
            &registries,
            &mut state,
            material_source,
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(1),
        ) {
            panic!("production-stale material seeding failed: {error}");
        }
        let inputs =
            match validate_process_inputs(&registries, &state, TEST_PROCESS, material_source) {
                Ok(inputs) => inputs,
                Err(error) => panic!("production-stale process input binding failed: {error}"),
            };
        let resolution = make_test_process_resolution(
            inputs,
            2,
            vec![MaterialLotSpec::new(
                CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
                Mass::from_milligrams(1),
                Temperature::from_millikelvin(293_150),
            )],
        );
        let start = match validate_start_process(
            &registries,
            &state,
            &resolution,
            material_source,
            material_destination,
        ) {
            Ok(start) => start,
            Err(error) => panic!("production-stale process start validation failed: {error}"),
        };
        if let Err(error) = start.commit(&mut state) {
            panic!("production-stale process start commit failed: {error}");
        }
        assert_eq!(state.energy().revision(), expected_energy_revision);
        assert_eq!(
            state.production().revision(),
            expected_production_revision + 1
        );
        let before_commit = state.clone();

        assert_eq!(
            validated.commit(&mut state),
            Err(EnergyTransferCommitError::StaleProductionRevision {
                expected: expected_production_revision,
                actual: expected_production_revision + 1,
            })
        );
        assert_eq!(state, before_commit);
    }

    #[test]
    fn repeated_energy_transfer_preserves_conservation_audits_and_replay() {
        let seed = WorldSeed::new(0x9310_0006);
        let first = run_transfer_soak(seed);
        let second = run_transfer_soak(seed);
        assert_eq!(first, second);
    }
}
