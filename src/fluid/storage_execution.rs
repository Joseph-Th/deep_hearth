//! Canonical finite fluid-store allocation and revision-bound transfer; pressure and path resolution remain separate sibling concerns.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Temperature, Volume};
use crate::core::state::AppState;
use crate::registry::Registries;
use crate::structural::{StructuralAnalysis, StructuralCommitError, StructuralMutationOutcome};

use super::definitions::FluidDefinitionId;
use super::state::{FluidContents, FluidStoreId, FluidStoreRecord};
use super::structural_integration::{
    FluidContentsChange, FluidStructuralLoadError, ValidatedFluidStructuralLoad,
    validate_fluid_contents_changes,
};

/// Failure while allocating one authoritative finite fluid store for controlled fixtures.
#[cfg(any(test, feature = "test-gameplay"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AddFluidStoreError {
    ZeroCapacity,
    UnknownDefinition { definition: FluidDefinitionId },
    InitialVolumeZero,
    InitialVolumeExceedsCapacity { initial: Volume, capacity: Volume },
    IdExhausted,
    RevisionExhausted,
}

#[cfg(any(test, feature = "test-gameplay"))]
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

#[cfg(any(test, feature = "test-gameplay"))]
impl Error for AddFluidStoreError {}

/// Allocates one empty finite fluid store for unit tests only.
#[cfg(test)]
pub(crate) fn add_fluid_store(
    state: &mut AppState,
    capacity: Volume,
) -> Result<FluidStoreId, AddFluidStoreError> {
    allocate_fluid_store(state, capacity, None)
}

#[cfg(any(test, feature = "test-gameplay"))]
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
    let fluid = state.fluid();
    let id = FluidStoreId::new(fluid.next_store_id());
    let next_store_id = fluid
        .next_store_id()
        .checked_add(1)
        .ok_or(AddFluidStoreError::IdExhausted)?;
    let next_revision = fluid
        .revision()
        .checked_add(1)
        .ok_or(AddFluidStoreError::RevisionExhausted)?;
    let record = FluidStoreRecord {
        id,
        capacity,
        contents,
        supported_by: None,
        created_at: state.tick(),
    };

    let fluid = state.fluid_state_mut();
    fluid.insert_store(record, next_store_id, next_revision);
    Ok(id)
}

#[cfg(any(test, feature = "test-gameplay"))]
pub(crate) fn add_fluid_store_with_contents_for_fixture(
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
#[derive(Debug, PartialEq, Eq)]
pub struct FluidTransferResolution {
    source: FluidStoreId,
    destination: FluidStoreId,
    volume: Volume,
}

impl FluidTransferResolution {
    #[must_use]
    pub const fn source(&self) -> FluidStoreId {
        self.source
    }

    #[must_use]
    pub const fn destination(&self) -> FluidStoreId {
        self.destination
    }

    #[must_use]
    pub const fn volume(&self) -> Volume {
        self.volume
    }
}

#[cfg(test)]
pub(crate) fn make_test_fluid_transfer_resolution(
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
#[derive(Clone, Debug, PartialEq, Eq)]
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
    UnknownFluidDefinition {
        definition: FluidDefinitionId,
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
    StructuralLoad(FluidStructuralLoadError),
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
            Self::UnknownFluidDefinition { definition } => write!(
                formatter,
                "fluid transfer references unknown fluid definition {}",
                definition.value()
            ),
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
            Self::StructuralLoad(error) => {
                write!(formatter, "fluid transfer structural load failed: {error}")
            }
            Self::RevisionExhausted => {
                formatter.write_str("fluid state revision space is exhausted")
            }
        }
    }
}

impl Error for FluidTransferError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::StructuralLoad(error) => Some(error),
            Self::SameStore { store: _store }
            | Self::UnknownSource { store: _store }
            | Self::UnknownDestination { store: _store }
            | Self::SourceEmpty { store: _store } => None,
            Self::UnknownFluidDefinition {
                definition: _definition,
            } => None,
            Self::InsufficientSourceVolume {
                store: _store,
                available: _available,
                requested: _requested,
            } => None,
            Self::DestinationFluidMismatch {
                destination: _destination,
                stored: _stored,
                incoming: _incoming,
            } => None,
            Self::DestinationTemperatureMismatch {
                destination: _destination,
                stored: _stored,
                incoming: _incoming,
            } => None,
            Self::DestinationVolumeOverflow {
                destination: _destination,
            } => None,
            Self::DestinationCapacityExceeded {
                destination: _destination,
                capacity: _capacity,
                stored: _stored,
                requested: _requested,
            } => None,
            Self::ZeroVolume | Self::RevisionExhausted => None,
        }
    }
}

/// Consumed proof that one resolved fluid transfer can commit against an exact owner revision.
#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub struct ValidatedFluidTransfer {
    expected_revision: u64,
    next_revision: u64,
    resolution: FluidTransferResolution,
    source_before: FluidContents,
    destination_before: Option<FluidContents>,
    source_after: Option<FluidContents>,
    destination_after: FluidContents,
    structural: Option<ValidatedFluidStructuralLoad>,
}

impl ValidatedFluidTransfer {
    /// Returns the structural consequence precomputed from both stores' final contents, when any
    /// affected support load changes.
    #[must_use]
    pub fn structural_analysis(&self) -> Option<&StructuralAnalysis> {
        self.structural
            .as_ref()
            .and_then(ValidatedFluidStructuralLoad::analysis)
    }
}

/// Validates all store, identity, thermal, quantity, and capacity constraints before mutation.
pub fn validate_fluid_transfer(
    registries: &Registries,
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
    if registries
        .fluid()
        .get_fluid(source_before.fluid())
        .is_none()
    {
        return Err(FluidTransferError::UnknownFluidDefinition {
            definition: source_before.fluid(),
        });
    }
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
    let source_after_volume = source_before.volume().checked_sub(volume).ok_or(
        FluidTransferError::InsufficientSourceVolume {
            store: source,
            available: source_before.volume(),
            requested: volume,
        },
    )?;
    let source_after = if source_after_volume.is_zero() {
        None
    } else {
        Some(FluidContents {
            fluid: source_before.fluid(),
            volume: source_after_volume,
            temperature: source_before.temperature(),
        })
    };
    let destination_after = FluidContents {
        fluid: source_before.fluid(),
        volume: destination_after,
        temperature: source_before.temperature(),
    };
    let structural = validate_fluid_contents_changes(
        registries,
        state,
        [
            FluidContentsChange::new(source, source_after),
            FluidContentsChange::new(destination, Some(destination_after)),
        ],
    )
    .map_err(FluidTransferError::StructuralLoad)?;
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
        source_after,
        destination_after,
        structural,
    })
}

/// Failure when a validated fluid transfer no longer matches its exact owner snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FluidTransferCommitError {
    StaleRevision { expected: u64, actual: u64 },
    SourceChanged { store: FluidStoreId },
    DestinationChanged { store: FluidStoreId },
    Structure(StructuralCommitError),
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
            Self::Structure(error) => write!(
                formatter,
                "fluid transfer structural commit failed: {error}"
            ),
        }
    }
}

impl Error for FluidTransferCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Structure(error) => Some(error),
            Self::StaleRevision {
                expected: _expected,
                actual: _actual,
            } => None,
            Self::SourceChanged { store: _store } | Self::DestinationChanged { store: _store } => {
                None
            }
        }
    }
}

/// Observable outcome of one committed exact fluid transfer.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FluidTransferOutcome {
    source: FluidStoreId,
    destination: FluidStoreId,
    fluid: FluidDefinitionId,
    volume: Volume,
    temperature: Temperature,
    structural: Option<StructuralMutationOutcome>,
}

impl FluidTransferOutcome {
    #[must_use]
    pub const fn source(&self) -> FluidStoreId {
        self.source
    }

    #[must_use]
    pub const fn destination(&self) -> FluidStoreId {
        self.destination
    }

    #[must_use]
    pub const fn fluid(&self) -> FluidDefinitionId {
        self.fluid
    }

    #[must_use]
    pub const fn volume(&self) -> Volume {
        self.volume
    }

    #[must_use]
    pub const fn temperature(&self) -> Temperature {
        self.temperature
    }

    /// Structural analysis produced by the fluid-weight change, when any load value changed.
    #[must_use]
    pub fn structural_analysis(&self) -> Option<&StructuralAnalysis> {
        self.structural
            .as_ref()
            .map(StructuralMutationOutcome::analysis)
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
            source_after,
            destination_after,
            structural,
        } = self;
        let FluidTransferResolution {
            source,
            destination,
            volume,
        } = resolution;
        {
            let fluid = state.fluid();
            if fluid.revision() != expected_revision {
                return Err(FluidTransferCommitError::StaleRevision {
                    expected: expected_revision,
                    actual: fluid.revision(),
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
        }
        let structural = match structural {
            Some(structural) => structural
                .commit(state)
                .map_err(FluidTransferCommitError::Structure)?,
            None => None,
        };

        let fluid = state.fluid_state_mut();
        fluid.apply_transfer_contents(
            source,
            source_after,
            destination,
            destination_after,
            next_revision,
        );

        Ok(FluidTransferOutcome {
            source,
            destination,
            fluid: source_before.fluid,
            volume,
            temperature: source_before.temperature,
            structural,
        })
    }
}

#[cfg(test)]
#[path = "storage_execution_tests.rs"]
mod tests;
