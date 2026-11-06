//! Typed authorization and commit failures for player-local fluid transfer.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Temperature, Volume};
use crate::fluid::structural_integration::FluidStructuralLoadError;
use crate::fluid::{FluidDefinitionId, FluidStoreId};
use crate::logistics::PlayerFluidStoreAccessError;
use crate::spatial::VoxelCoord;
use crate::structural::StructuralCommitError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FluidTransferError {
    PlayerNotAdmitted,
    PlayerDead,
    PlayerBusy,
    PreparedStrikeActive,
    SourceAccess(PlayerFluidStoreAccessError),
    DestinationAccess(PlayerFluidStoreAccessError),
    InvalidGravityRoute {
        source_position: VoxelCoord,
        destination_position: VoxelCoord,
    },
    BlockedGravityDestination {
        destination_position: VoxelCoord,
    },
    UnknownGravityTerrain {
        destination_position: VoxelCoord,
    },
    /// A previously selected choice now has different resulting contents.
    ObservedOutcomeChanged {
        source: FluidStoreId,
        destination: FluidStoreId,
    },
    UnknownStore {
        store: FluidStoreId,
    },
    SameStore,
    ZeroVolume,
    EmptySource {
        store: FluidStoreId,
    },
    InsufficientVolume {
        available: Volume,
        requested: Volume,
    },
    UnknownDefinition {
        definition: FluidDefinitionId,
    },
    IncompatibleFluid {
        expected: FluidDefinitionId,
        found: FluidDefinitionId,
    },
    UnsafeHandTemperature {
        store: FluidStoreId,
        temperature: Temperature,
        maximum: Temperature,
    },
    UnrepresentableEquilibriumTemperature {
        weighted_millikelvin_microliters: u128,
        combined_volume: Volume,
    },
    DestinationOverflow,
    InsufficientCapacity {
        available: Volume,
        requested: Volume,
    },
    RevisionExhausted,
    StructuralLoad(FluidStructuralLoadError),
}

impl Display for FluidTransferError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlayerNotAdmitted => {
                formatter.write_str("fluid transfer requires an admitted player")
            }
            Self::PlayerDead => formatter.write_str("dead player cannot transfer fluid"),
            Self::PlayerBusy => {
                formatter.write_str("player cannot transfer fluid while work is active")
            }
            Self::PreparedStrikeActive => formatter
                .write_str("player must lower the prepared weapon before transferring fluid"),
            Self::SourceAccess(error) => write!(
                formatter,
                "source fluid store is not locally accessible: {error}"
            ),
            Self::DestinationAccess(error) => write!(
                formatter,
                "destination fluid store is not locally accessible: {error}"
            ),
            Self::InvalidGravityRoute {
                source_position,
                destination_position,
            } => write!(
                formatter,
                "gravity feed must go directly down one voxel from ({},{},{}) to the lower store; destination is at ({},{},{})",
                source_position.x(),
                source_position.y(),
                source_position.z(),
                destination_position.x(),
                destination_position.y(),
                destination_position.z()
            ),
            Self::BlockedGravityDestination {
                destination_position,
            } => write!(
                formatter,
                "gravity feed destination ({},{},{}) is obstructed",
                destination_position.x(),
                destination_position.y(),
                destination_position.z()
            ),
            Self::UnknownGravityTerrain {
                destination_position,
            } => write!(
                formatter,
                "gravity feed destination ({},{},{}) has unavailable terrain authority",
                destination_position.x(),
                destination_position.y(),
                destination_position.z()
            ),
            Self::ObservedOutcomeChanged {
                source,
                destination,
            } => write!(
                formatter,
                "observed transfer between fluid stores {} and {} is no longer current",
                source.value(),
                destination.value()
            ),
            Self::UnknownStore { store } => {
                write!(formatter, "fluid store {} does not exist", store.value())
            }
            Self::SameStore => formatter.write_str("fluid transfer requires two distinct stores"),
            Self::ZeroVolume => formatter.write_str("fluid transfer volume must be nonzero"),
            Self::EmptySource { store } => {
                write!(formatter, "source fluid store {} is empty", store.value())
            }
            Self::InsufficientVolume {
                available,
                requested,
            } => write!(
                formatter,
                "fluid source holds {} µL but {} µL was requested",
                available.microliters(),
                requested.microliters()
            ),
            Self::UnknownDefinition { definition } => write!(
                formatter,
                "fluid definition {} does not exist",
                definition.value()
            ),
            Self::IncompatibleFluid { expected, found } => write!(
                formatter,
                "cannot mix fluid definition {} with fluid definition {}",
                found.value(),
                expected.value()
            ),
            Self::UnsafeHandTemperature {
                store,
                temperature,
                maximum,
            } => write!(
                formatter,
                "fluid store {} is at {} mK, above the unprotected handling limit of {} mK",
                store.value(),
                temperature.millikelvin(),
                maximum.millikelvin()
            ),
            Self::UnrepresentableEquilibriumTemperature {
                weighted_millikelvin_microliters,
                combined_volume,
            } => write!(
                formatter,
                "mixing would require sub-millikelvin rounding: {weighted_millikelvin_microliters} mK·µL across {} µL",
                combined_volume.microliters()
            ),
            Self::DestinationOverflow => {
                formatter.write_str("fluid destination volume would overflow")
            }
            Self::InsufficientCapacity {
                available,
                requested,
            } => write!(
                formatter,
                "fluid destination has {} µL free but {} µL was requested",
                available.microliters(),
                requested.microliters()
            ),
            Self::RevisionExhausted => formatter.write_str("fluid revision space is exhausted"),
            Self::StructuralLoad(error) => write!(
                formatter,
                "fluid transfer cannot satisfy structural load: {error}"
            ),
        }
    }
}
impl Error for FluidTransferError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::SourceAccess(error) | Self::DestinationAccess(error) => Some(error),
            Self::StructuralLoad(error) => Some(error),
            Self::PlayerNotAdmitted
            | Self::PlayerDead
            | Self::PlayerBusy
            | Self::PreparedStrikeActive
            | Self::InvalidGravityRoute { .. }
            | Self::BlockedGravityDestination { .. }
            | Self::UnknownGravityTerrain { .. }
            | Self::ObservedOutcomeChanged { .. }
            | Self::UnknownStore { .. }
            | Self::SameStore
            | Self::ZeroVolume
            | Self::EmptySource { .. }
            | Self::InsufficientVolume { .. }
            | Self::UnknownDefinition { .. }
            | Self::IncompatibleFluid { .. }
            | Self::UnsafeHandTemperature { .. }
            | Self::UnrepresentableEquilibriumTemperature { .. }
            | Self::DestinationOverflow
            | Self::InsufficientCapacity { .. }
            | Self::RevisionExhausted => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FluidTransferCommitError {
    StaleFluidRevision { expected: u64, actual: u64 },
    StaleWorldRevision { expected: u64, actual: u64 },
    StaleStructureRevision { expected: u64, actual: u64 },
    StaleLogisticsRevision { expected: u64, actual: u64 },
    StaleSurvivalRevision { expected: u64, actual: u64 },
    StalePlayerWorkRevision { expected: u64, actual: u64 },
    StaleCombatRevision { expected: u64, actual: u64 },
    ContentsChanged { store: FluidStoreId },
    Structure(StructuralCommitError),
}

impl Display for FluidTransferCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleFluidRevision { expected, actual } => write!(
                formatter,
                "fluid revision changed from {expected} to {actual} before transfer commit"
            ),
            Self::StaleWorldRevision { expected, actual } => write!(
                formatter,
                "world revision changed from {expected} to {actual} before transfer commit"
            ),
            Self::StaleStructureRevision { expected, actual } => write!(
                formatter,
                "structure revision changed from {expected} to {actual} before transfer commit"
            ),
            Self::StaleLogisticsRevision { expected, actual } => write!(
                formatter,
                "logistics revision changed from {expected} to {actual} before transfer commit"
            ),
            Self::StaleSurvivalRevision { expected, actual } => write!(
                formatter,
                "survival revision changed from {expected} to {actual} before transfer commit"
            ),
            Self::StalePlayerWorkRevision { expected, actual } => write!(
                formatter,
                "player work revision changed from {expected} to {actual} before transfer commit"
            ),
            Self::StaleCombatRevision { expected, actual } => write!(
                formatter,
                "combat revision changed from {expected} to {actual} before transfer commit"
            ),
            Self::ContentsChanged { store } => write!(
                formatter,
                "fluid store {} contents changed before transfer commit",
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
            Self::StaleFluidRevision { .. }
            | Self::StaleWorldRevision { .. }
            | Self::StaleStructureRevision { .. }
            | Self::StaleLogisticsRevision { .. }
            | Self::StaleSurvivalRevision { .. }
            | Self::StalePlayerWorkRevision { .. }
            | Self::StaleCombatRevision { .. }
            | Self::ContentsChanged { .. } => None,
        }
    }
}
