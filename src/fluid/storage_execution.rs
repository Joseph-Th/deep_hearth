//! Canonical finite fluid-store allocation and revision-bound transfer; pressure and path resolution remain separate sibling concerns.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Temperature, Volume};
use crate::core::state::AppState;
#[cfg(test)]
use crate::registry::Registries;

use super::definitions::FluidDefinitionId;
use super::state::{FluidContents, FluidStoreId, FluidStoreRecord};

/// Failure while allocating one authoritative finite fluid store.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddFluidStoreError {
    ZeroCapacity,
    UnknownDefinition { definition: FluidDefinitionId },
    InitialVolumeZero,
    InitialVolumeExceedsCapacity { initial: Volume, capacity: Volume },
    IdExhausted,
    RevisionExhausted,
}

impl Display for AddFluidStoreError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroCapacity => formatter.write_str("fluid store capacity must be nonzero"),
            Self::UnknownDefinition { definition } => {
                write!(formatter, "unknown fluid definition {}", definition.value())
            }
            Self::InitialVolumeZero => formatter.write_str("initial fluid volume must be nonzero"),
            Self::InitialVolumeExceedsCapacity { initial, capacity } => write!(
                formatter,
                "initial fluid volume {} uL exceeds store capacity {} uL",
                initial.microliters(),
                capacity.microliters()
            ),
            Self::IdExhausted => formatter.write_str("fluid store identifier space is exhausted"),
            Self::RevisionExhausted => {
                formatter.write_str("fluid state revision space is exhausted")
            }
        }
    }
}

impl Error for AddFluidStoreError {}

/// Allocates one empty finite fluid store without creating fluid.
pub fn add_fluid_store(
    state: &mut AppState,
    capacity: Volume,
) -> Result<FluidStoreId, AddFluidStoreError> {
    allocate_fluid_store(state, capacity, None)
}

fn allocate_fluid_store(
    state: &mut AppState,
    capacity: Volume,
    contents: Option<FluidContents>,
) -> Result<FluidStoreId, AddFluidStoreError> {
    if capacity.is_zero() {
        return Err(AddFluidStoreError::ZeroCapacity);
    }
    if let Some(contents) = contents {
        if contents.volume.is_zero() {
            return Err(AddFluidStoreError::InitialVolumeZero);
        }
        if contents.volume > capacity {
            return Err(AddFluidStoreError::InitialVolumeExceedsCapacity {
                initial: contents.volume,
                capacity,
            });
        }
    }
    let fluid = state.fluid_state();
    let id = FluidStoreId::new(fluid.next_store_id);
    let next_store_id = fluid
        .next_store_id
        .checked_add(1)
        .ok_or(AddFluidStoreError::IdExhausted)?;
    let next_revision = fluid
        .revision
        .checked_add(1)
        .ok_or(AddFluidStoreError::RevisionExhausted)?;
    let record = FluidStoreRecord {
        id,
        capacity,
        contents,
        created_at: state.tick(),
    };

    let fluid = state.fluid_state_mut();
    let previous = fluid.records.insert(id, record);
    debug_assert!(
        previous.is_none(),
        "Runtime Invariant 4 (Index Uniqueness): fluid store allocation replaced an existing record"
    );
    fluid.next_store_id = next_store_id;
    fluid.revision = next_revision;
    Ok(id)
}

#[cfg(test)]
pub(crate) fn add_fluid_store_with_contents_for_test(
    registries: &Registries,
    state: &mut AppState,
    capacity: Volume,
    definition: FluidDefinitionId,
    volume: Volume,
    temperature: Temperature,
) -> Result<FluidStoreId, AddFluidStoreError> {
    if registries.fluid().get_fluid(definition).is_none() {
        return Err(AddFluidStoreError::UnknownDefinition { definition });
    }
    allocate_fluid_store(
        state,
        capacity,
        Some(FluidContents {
            fluid: definition,
            volume,
            temperature,
        }),
    )
}

/// Immutable output of a future pressure, gravity, channel, or pump resolver.
///
/// There is no public constructor. The storage owner proves only that an already-resolved transfer
/// is still valid and commits it atomically; it does not authorize teleporting fluid between stores.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FluidTransferResolution {
    source: FluidStoreId,
    destination: FluidStoreId,
    volume: Volume,
}

impl FluidTransferResolution {
    #[must_use]
    pub const fn source(self) -> FluidStoreId {
        self.source
    }

    #[must_use]
    pub const fn destination(self) -> FluidStoreId {
        self.destination
    }

    #[must_use]
    pub const fn volume(self) -> Volume {
        self.volume
    }
}

#[cfg(test)]
fn make_test_fluid_transfer_resolution(
    source: FluidStoreId,
    destination: FluidStoreId,
    volume: Volume,
) -> FluidTransferResolution {
    FluidTransferResolution {
        source,
        destination,
        volume,
    }
}

/// Failure while validating one already physically resolved fluid transfer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FluidTransferError {
    SameStore {
        store: FluidStoreId,
    },
    ZeroVolume,
    UnknownSource {
        store: FluidStoreId,
    },
    UnknownDestination {
        store: FluidStoreId,
    },
    SourceEmpty {
        store: FluidStoreId,
    },
    InsufficientSourceVolume {
        store: FluidStoreId,
        available: Volume,
        requested: Volume,
    },
    DestinationFluidMismatch {
        destination: FluidStoreId,
        stored: FluidDefinitionId,
        incoming: FluidDefinitionId,
    },
    DestinationTemperatureMismatch {
        destination: FluidStoreId,
        stored: Temperature,
        incoming: Temperature,
    },
    DestinationVolumeOverflow {
        destination: FluidStoreId,
    },
    DestinationCapacityExceeded {
        destination: FluidStoreId,
        capacity: Volume,
        stored: Volume,
        requested: Volume,
    },
    RevisionExhausted,
}

impl Display for FluidTransferError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SameStore { store } => write!(
                formatter,
                "fluid store {} cannot transfer into itself",
                store.value()
            ),
            Self::ZeroVolume => formatter.write_str("fluid transfer volume must be nonzero"),
            Self::UnknownSource { store } => {
                write!(formatter, "unknown source fluid store {}", store.value())
            }
            Self::UnknownDestination { store } => write!(
                formatter,
                "unknown destination fluid store {}",
                store.value()
            ),
            Self::SourceEmpty { store } => {
                write!(formatter, "source fluid store {} is empty", store.value())
            }
            Self::InsufficientSourceVolume {
                store,
                available,
                requested,
            } => write!(
                formatter,
                "source fluid store {} contains {} uL but transfer requires {} uL",
                store.value(),
                available.microliters(),
                requested.microliters()
            ),
            Self::DestinationFluidMismatch {
                destination,
                stored,
                incoming,
            } => write!(
                formatter,
                "fluid store {} contains fluid {} and cannot implicitly mix incoming fluid {}",
                destination.value(),
                stored.value(),
                incoming.value()
            ),
            Self::DestinationTemperatureMismatch {
                destination,
                stored,
                incoming,
            } => write!(
                formatter,
                "fluid store {} contains fluid at {} mK and cannot implicitly mix incoming fluid at {} mK",
                destination.value(),
                stored.millikelvin(),
                incoming.millikelvin()
            ),
            Self::DestinationVolumeOverflow { destination } => write!(
                formatter,
                "fluid transfer overflows destination store {} volume accounting",
                destination.value()
            ),
            Self::DestinationCapacityExceeded {
                destination,
                capacity,
                stored,
                requested,
            } => write!(
                formatter,
                "fluid store {} capacity {} uL cannot accept {} uL with {} uL already stored",
                destination.value(),
                capacity.microliters(),
                requested.microliters(),
                stored.microliters()
            ),
            Self::RevisionExhausted => {
                formatter.write_str("fluid state revision space is exhausted")
            }
        }
    }
}

impl Error for FluidTransferError {}

/// Consumed proof that one resolved fluid transfer can commit against an exact owner revision.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValidatedFluidTransfer {
    expected_revision: u64,
    next_revision: u64,
    resolution: FluidTransferResolution,
    source_before: FluidContents,
    destination_before: Option<FluidContents>,
}

/// Validates all store, identity, thermal, quantity, and capacity constraints before mutation.
pub fn validate_fluid_transfer(
    state: &AppState,
    resolution: FluidTransferResolution,
) -> Result<ValidatedFluidTransfer, FluidTransferError> {
    let FluidTransferResolution {
        source,
        destination,
        volume,
    } = resolution;
    if source == destination {
        return Err(FluidTransferError::SameStore { store: source });
    }
    if volume.is_zero() {
        return Err(FluidTransferError::ZeroVolume);
    }
    let fluid = state.fluid();
    let source_record = fluid
        .get_store(source)
        .ok_or(FluidTransferError::UnknownSource { store: source })?;
    let destination_record = fluid
        .get_store(destination)
        .ok_or(FluidTransferError::UnknownDestination { store: destination })?;
    let source_before = source_record
        .contents()
        .ok_or(FluidTransferError::SourceEmpty { store: source })?;
    if source_before.volume() < volume {
        return Err(FluidTransferError::InsufficientSourceVolume {
            store: source,
            available: source_before.volume(),
            requested: volume,
        });
    }
    let destination_before = destination_record.contents();
    if let Some(stored) = destination_before {
        if stored.fluid() != source_before.fluid() {
            return Err(FluidTransferError::DestinationFluidMismatch {
                destination,
                stored: stored.fluid(),
                incoming: source_before.fluid(),
            });
        }
        if stored.temperature() != source_before.temperature() {
            return Err(FluidTransferError::DestinationTemperatureMismatch {
                destination,
                stored: stored.temperature(),
                incoming: source_before.temperature(),
            });
        }
    }
    let destination_stored = destination_record.stored_volume();
    let destination_after = destination_stored
        .checked_add(volume)
        .ok_or(FluidTransferError::DestinationVolumeOverflow { destination })?;
    if destination_after > destination_record.capacity() {
        return Err(FluidTransferError::DestinationCapacityExceeded {
            destination,
            capacity: destination_record.capacity(),
            stored: destination_stored,
            requested: volume,
        });
    }
    let next_revision = fluid
        .revision()
        .checked_add(1)
        .ok_or(FluidTransferError::RevisionExhausted)?;
    Ok(ValidatedFluidTransfer {
        expected_revision: fluid.revision(),
        next_revision,
        resolution,
        source_before,
        destination_before,
    })
}

/// Failure when a validated fluid transfer no longer matches its exact owner snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FluidTransferCommitError {
    StaleRevision { expected: u64, actual: u64 },
    SourceChanged { store: FluidStoreId },
    DestinationChanged { store: FluidStoreId },
}

impl Display for FluidTransferCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleRevision { expected, actual } => write!(
                formatter,
                "validated fluid transfer expected revision {expected} but current revision is {actual}"
            ),
            Self::SourceChanged { store } => write!(
                formatter,
                "fluid transfer source {} changed without the validated owner revision",
                store.value()
            ),
            Self::DestinationChanged { store } => write!(
                formatter,
                "fluid transfer destination {} changed without the validated owner revision",
                store.value()
            ),
        }
    }
}

impl Error for FluidTransferCommitError {}

/// Observable outcome of one committed exact fluid transfer.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FluidTransferOutcome {
    source: FluidStoreId,
    destination: FluidStoreId,
    fluid: FluidDefinitionId,
    volume: Volume,
    temperature: Temperature,
}

impl FluidTransferOutcome {
    #[must_use]
    pub const fn source(self) -> FluidStoreId {
        self.source
    }

    #[must_use]
    pub const fn destination(self) -> FluidStoreId {
        self.destination
    }

    #[must_use]
    pub const fn fluid(self) -> FluidDefinitionId {
        self.fluid
    }

    #[must_use]
    pub const fn volume(self) -> Volume {
        self.volume
    }

    #[must_use]
    pub const fn temperature(self) -> Temperature {
        self.temperature
    }
}

impl ValidatedFluidTransfer {
    /// Commits this transfer exactly once after rechecking the owner revision and snapshots.
    pub fn commit(
        self,
        state: &mut AppState,
    ) -> Result<FluidTransferOutcome, FluidTransferCommitError> {
        let Self {
            expected_revision,
            next_revision,
            resolution,
            source_before,
            destination_before,
        } = self;
        let FluidTransferResolution {
            source,
            destination,
            volume,
        } = resolution;
        let fluid = state.fluid_state();
        if fluid.revision != expected_revision {
            return Err(FluidTransferCommitError::StaleRevision {
                expected: expected_revision,
                actual: fluid.revision,
            });
        }
        if fluid.get_store(source).and_then(FluidStoreRecord::contents) != Some(source_before) {
            return Err(FluidTransferCommitError::SourceChanged { store: source });
        }
        if fluid
            .get_store(destination)
            .and_then(FluidStoreRecord::contents)
            != destination_before
        {
            return Err(FluidTransferCommitError::DestinationChanged { store: destination });
        }

        let source_after_volume = match source_before.volume.checked_sub(volume) {
            Some(value) => value,
            None => unreachable!("validated fluid source volume cannot underflow"),
        };
        let destination_after_volume = match destination_before {
            Some(contents) => match contents.volume.checked_add(volume) {
                Some(value) => value,
                None => unreachable!("validated fluid destination volume cannot overflow"),
            },
            None => volume,
        };
        let destination_after = FluidContents {
            fluid: source_before.fluid,
            volume: destination_after_volume,
            temperature: source_before.temperature,
        };

        let fluid = state.fluid_state_mut();
        let Some(source_record) = fluid.records.get_mut(&source) else {
            unreachable!("validated fluid source cannot disappear without a revision change");
        };
        source_record.contents = if source_after_volume.is_zero() {
            None
        } else {
            Some(FluidContents {
                fluid: source_before.fluid,
                volume: source_after_volume,
                temperature: source_before.temperature,
            })
        };
        let Some(destination_record) = fluid.records.get_mut(&destination) else {
            unreachable!("validated fluid destination cannot disappear without a revision change");
        };
        destination_record.contents = Some(destination_after);
        fluid.revision = next_revision;

        Ok(FluidTransferOutcome {
            source,
            destination,
            fluid: source_before.fluid,
            volume,
            temperature: source_before.temperature,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{MATERIAL_COPPER, MATERIAL_SLAG, make_test_registries_with_fluids};
    use crate::core::quantity::AggregateVolume;
    use crate::core::state::StateValidationError;
    use crate::core::time::WorldSeed;
    use crate::fluid::{FluidDefinition, FluidValidationError, calculate_fluid_volume_accounting};
    use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};

    const WATER_LIKE: FluidDefinitionId = FluidDefinitionId::new(940_001);
    const OIL_LIKE: FluidDefinitionId = FluidDefinitionId::new(940_002);
    const TEMPERATURE: Temperature = Temperature::from_millikelvin(293_150);

    fn registries() -> Registries {
        make_test_registries_with_fluids(vec![
            FluidDefinition::new(WATER_LIKE, "fluid transfer fixture A", MATERIAL_COPPER),
            FluidDefinition::new(OIL_LIKE, "fluid transfer fixture B", MATERIAL_SLAG),
        ])
    }

    fn add_filled(
        registries: &Registries,
        state: &mut AppState,
        capacity: u64,
        fluid: FluidDefinitionId,
        volume: u64,
        temperature: Temperature,
    ) -> FluidStoreId {
        match add_fluid_store_with_contents_for_test(
            registries,
            state,
            Volume::from_microliters(capacity),
            fluid,
            Volume::from_microliters(volume),
            temperature,
        ) {
            Ok(store) => store,
            Err(error) => panic!("fluid fixture allocation failed: {error}"),
        }
    }

    fn commit_transfer(
        state: &mut AppState,
        source: FluidStoreId,
        destination: FluidStoreId,
        volume: u64,
    ) -> FluidTransferOutcome {
        let resolution = make_test_fluid_transfer_resolution(
            source,
            destination,
            Volume::from_microliters(volume),
        );
        let token = match validate_fluid_transfer(state, resolution) {
            Ok(token) => token,
            Err(error) => panic!("fluid transfer validation failed: {error}"),
        };
        match token.commit(state) {
            Ok(outcome) => outcome,
            Err(error) => panic!("fluid transfer commit failed: {error}"),
        }
    }

    #[test]
    fn runtime_allocation_creates_empty_capacity_without_free_fluid() {
        let mut state = AppState::new(WorldSeed::new(0x9400_0001));
        let store = match add_fluid_store(&mut state, Volume::from_microliters(500)) {
            Ok(store) => store,
            Err(error) => panic!("fluid store allocation failed: {error}"),
        };

        assert_eq!(
            state
                .fluid()
                .get_store(store)
                .and_then(FluidStoreRecord::contents),
            None
        );
        assert_eq!(state.fluid().revision(), 1);
    }

    #[test]
    fn validated_transfer_moves_exact_volume_and_clears_empty_source_identity() {
        let registries = registries();
        let mut state = AppState::new(WorldSeed::new(0x9400_0002));
        let source = add_filled(&registries, &mut state, 500, WATER_LIKE, 275, TEMPERATURE);
        let destination = match add_fluid_store(&mut state, Volume::from_microliters(500)) {
            Ok(store) => store,
            Err(error) => panic!("destination allocation failed: {error}"),
        };
        let before = calculate_fluid_volume_accounting(&state);

        let outcome = commit_transfer(&mut state, source, destination, 275);

        assert_eq!(outcome.fluid(), WATER_LIKE);
        assert_eq!(outcome.volume(), Volume::from_microliters(275));
        assert_eq!(outcome.temperature(), TEMPERATURE);
        assert_eq!(
            state
                .fluid()
                .get_store(source)
                .and_then(FluidStoreRecord::contents),
            None
        );
        assert_eq!(
            state
                .fluid()
                .get_store(destination)
                .and_then(FluidStoreRecord::contents)
                .map(FluidContents::volume),
            Some(Volume::from_microliters(275))
        );
        assert_eq!(calculate_fluid_volume_accounting(&state), before);
    }

    #[test]
    fn incompatible_identity_or_temperature_requires_a_separate_mixing_resolver() {
        let registries = registries();
        let mut state = AppState::new(WorldSeed::new(0x9400_0003));
        let source = add_filled(&registries, &mut state, 100, WATER_LIKE, 50, TEMPERATURE);
        let other_fluid = add_filled(&registries, &mut state, 100, OIL_LIKE, 10, TEMPERATURE);
        let hotter = add_filled(
            &registries,
            &mut state,
            100,
            WATER_LIKE,
            10,
            Temperature::from_millikelvin(303_150),
        );
        let before = state.clone();

        assert!(matches!(
            validate_fluid_transfer(
                &state,
                make_test_fluid_transfer_resolution(
                    source,
                    other_fluid,
                    Volume::from_microliters(1),
                ),
            ),
            Err(FluidTransferError::DestinationFluidMismatch { .. })
        ));
        assert!(matches!(
            validate_fluid_transfer(
                &state,
                make_test_fluid_transfer_resolution(source, hotter, Volume::from_microliters(1),),
            ),
            Err(FluidTransferError::DestinationTemperatureMismatch { .. })
        ));
        assert_eq!(state, before);
    }

    #[test]
    fn capacity_failure_and_stale_token_leave_fluid_unchanged() {
        let registries = registries();
        let mut state = AppState::new(WorldSeed::new(0x9400_0004));
        let source = add_filled(&registries, &mut state, 100, WATER_LIKE, 80, TEMPERATURE);
        let destination = add_filled(&registries, &mut state, 50, WATER_LIKE, 45, TEMPERATURE);
        let before_capacity_failure = state.clone();
        assert!(matches!(
            validate_fluid_transfer(
                &state,
                make_test_fluid_transfer_resolution(
                    source,
                    destination,
                    Volume::from_microliters(6),
                ),
            ),
            Err(FluidTransferError::DestinationCapacityExceeded { .. })
        ));
        assert_eq!(state, before_capacity_failure);

        let token = match validate_fluid_transfer(
            &state,
            make_test_fluid_transfer_resolution(source, destination, Volume::from_microliters(5)),
        ) {
            Ok(token) => token,
            Err(error) => panic!("stale-token setup failed: {error}"),
        };
        let extra = match add_fluid_store(&mut state, Volume::from_microliters(1)) {
            Ok(store) => store,
            Err(error) => panic!("independent fluid mutation failed: {error}"),
        };
        let before_commit = state.clone();
        assert_eq!(
            token.commit(&mut state),
            Err(FluidTransferCommitError::StaleRevision {
                expected: before_commit.fluid().revision() - 1,
                actual: before_commit.fluid().revision(),
            })
        );
        assert!(state.fluid().get_store(extra).is_some());
        assert_eq!(state, before_commit);
    }

    #[test]
    fn aggregate_volume_is_wider_than_one_store_and_transfer_conserves_it() {
        let registries = registries();
        let mut state = AppState::new(WorldSeed::new(0x9400_0005));
        let first = add_filled(
            &registries,
            &mut state,
            u64::MAX,
            WATER_LIKE,
            u64::MAX,
            TEMPERATURE,
        );
        let second = add_filled(
            &registries,
            &mut state,
            u64::MAX,
            WATER_LIKE,
            u64::MAX - 10,
            TEMPERATURE,
        );
        let third = match add_fluid_store(&mut state, Volume::from_microliters(100)) {
            Ok(store) => store,
            Err(error) => panic!("third store fixture failed: {error}"),
        };
        let initial = match calculate_fluid_volume_accounting(&state) {
            Ok(accounting) => accounting,
            Err(error) => panic!("fluid accounting failed: {error}"),
        };
        assert_eq!(
            initial.total(),
            AggregateVolume::from_microliters(u128::from(u64::MAX) * 2 - 10)
        );

        let _ = commit_transfer(&mut state, first, third, 10);
        let final_accounting = calculate_fluid_volume_accounting(&state);
        assert_eq!(final_accounting, Ok(initial));
        assert_eq!(
            state
                .fluid()
                .get_store(second)
                .map(FluidStoreRecord::stored_volume),
            Some(Volume::from_microliters(u64::MAX - 10))
        );
    }

    #[test]
    fn fluid_state_round_trip_preserves_exact_continuation() {
        let registries = registries();
        let mut state = AppState::new(WorldSeed::new(0x9400_0006));
        let source = add_filled(&registries, &mut state, 1_000, WATER_LIKE, 700, TEMPERATURE);
        let destination = match add_fluid_store(&mut state, Volume::from_microliters(1_000)) {
            Ok(store) => store,
            Err(error) => panic!("round-trip destination failed: {error}"),
        };
        let _ = commit_transfer(&mut state, source, destination, 125);

        let encoded = match serde_json::to_vec(&SaveEnvelope::new(&registries, &state)) {
            Ok(encoded) => encoded,
            Err(error) => panic!("fluid save serialization failed: {error}"),
        };
        let decoded: LoadedSaveEnvelope = match serde_json::from_slice(&encoded) {
            Ok(decoded) => decoded,
            Err(error) => panic!("fluid save deserialization failed: {error}"),
        };
        let mut loaded = match decoded.into_state(&registries) {
            Ok(state) => state,
            Err(error) => panic!("fluid save validation failed: {error}"),
        };
        assert_eq!(loaded, state);

        let _ = commit_transfer(&mut state, source, destination, 75);
        let _ = commit_transfer(&mut loaded, source, destination, 75);
        assert_eq!(loaded, state);
    }

    #[test]
    fn persisted_store_rejects_unknown_fluid_definition_reference() {
        let registries = registries();
        let mut state = AppState::new(WorldSeed::new(0x9400_0008));
        let store = add_filled(&registries, &mut state, 1_000, WATER_LIKE, 700, TEMPERATURE);
        let unknown = FluidDefinitionId::new(949_999);
        let mut encoded = match serde_json::to_value(SaveEnvelope::new(&registries, &state)) {
            Ok(encoded) => encoded,
            Err(error) => panic!("fluid validation save serialization failed: {error}"),
        };
        encoded["state"]["fluid"]["records"][store.value().to_string()]["contents"]["fluid"] =
            serde_json::json!(unknown.value());
        let decoded: LoadedSaveEnvelope = match serde_json::from_value(encoded) {
            Ok(decoded) => decoded,
            Err(error) => panic!("tampered fluid save failed decode: {error}"),
        };

        assert_eq!(
            decoded.into_state(&registries),
            Err(LoadError::InvalidState(StateValidationError::Fluid(
                FluidValidationError::UnknownDefinition {
                    store,
                    definition: unknown,
                }
            )))
        );
    }

    #[test]
    fn repeated_transfers_preserve_volume_and_deterministic_state() {
        let registries = registries();
        let mut first = AppState::new(WorldSeed::new(0x9400_0007));
        let source = add_filled(
            &registries,
            &mut first,
            10_000,
            WATER_LIKE,
            8_000,
            TEMPERATURE,
        );
        let middle = match add_fluid_store(&mut first, Volume::from_microliters(10_000)) {
            Ok(store) => store,
            Err(error) => panic!("middle store fixture failed: {error}"),
        };
        let destination = match add_fluid_store(&mut first, Volume::from_microliters(10_000)) {
            Ok(store) => store,
            Err(error) => panic!("destination store fixture failed: {error}"),
        };
        let initial = match calculate_fluid_volume_accounting(&first) {
            Ok(accounting) => accounting.total(),
            Err(error) => panic!("initial volume accounting failed: {error}"),
        };
        let mut second = first.clone();

        for step in 0..2_000_u64 {
            let (from, to) = match step % 4 {
                0 => (source, middle),
                1 => (middle, destination),
                2 => (destination, middle),
                3 => (middle, source),
                _ => unreachable!("modulo four must be exhaustive"),
            };
            let _ = commit_transfer(&mut first, from, to, 1);
            let _ = commit_transfer(&mut second, from, to, 1);
            if step % 127 == 0 {
                let accounting = match calculate_fluid_volume_accounting(&first) {
                    Ok(accounting) => accounting,
                    Err(error) => panic!("soak fluid accounting failed: {error}"),
                };
                assert_eq!(accounting.total(), initial);
                if let Err(error) = crate::core::state::validate_loaded_state(&registries, &first) {
                    panic!("soak fluid state validation failed: {error}");
                }
            }
        }

        assert_eq!(first, second);
        assert_eq!(
            calculate_fluid_volume_accounting(&first).map(|accounting| accounting.total()),
            Ok(initial)
        );
    }
}
