//! Typed admission and stale-commit failures for deterministic wilderness generation.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::time::{ControlTick, SimulationTick};
use crate::fluid::FluidStoreId;
use crate::geology::GeologicalDepositGenerationError;
use crate::spatial::VoxelCoord;
use crate::surface::SurfaceResourceValidationError;
use crate::world::WorldGenerationSeed;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InitialWildernessGenerationError {
    NotInitialControlTick {
        control_tick: ControlTick,
    },
    NotInitialTick {
        tick: SimulationTick,
    },
    PlayerAlreadyInitialized {
        survival_initialized: bool,
        logistics_initialized: bool,
    },
    SurfaceAlreadyGenerated,
    GeologyAlreadyGenerated,
    KoboldsAlreadyGenerated,
    FluidAlreadyGenerated,
    LogisticsAlreadyPopulated,
    TerrainSeedMismatch {
        established: WorldGenerationSeed,
        requested: WorldGenerationSeed,
    },
    SurfaceIdExhausted,
    SurfaceRevisionExhausted,
    FluidStoreIdExhausted,
    FluidRevisionExhausted,
    LogisticsRevisionExhausted,
    NoTraversableTerrainLayout,
    OutcropBoundsOverflow {
        position: VoxelCoord,
    },
    InvalidGeneratedResource(SurfaceResourceValidationError),
    InvalidGeneratedDeposit(GeologicalDepositGenerationError),
    InvalidGeneratedWater,
}

impl Display for InitialWildernessGenerationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotInitialControlTick { control_tick } => write!(
                formatter,
                "initial wilderness generation requires control tick 0, current control tick is {}",
                control_tick.value()
            ),
            Self::NotInitialTick { tick } => write!(
                formatter,
                "initial wilderness generation requires tick 0, current tick is {}",
                tick.value()
            ),
            Self::PlayerAlreadyInitialized {
                survival_initialized,
                logistics_initialized,
            } => write!(
                formatter,
                "initial wilderness generation requires an unadmitted player: survival initialized={survival_initialized}, logistics initialized={logistics_initialized}"
            ),
            Self::SurfaceAlreadyGenerated => formatter.write_str(
                "initial wilderness surface generation requires pristine surface-resource state",
            ),
            Self::GeologyAlreadyGenerated => formatter.write_str(
                "initial wilderness geological generation requires pristine geological state",
            ),
            Self::KoboldsAlreadyGenerated => formatter.write_str(
                "initial wilderness kobold generation requires pristine colony state",
            ),
            Self::FluidAlreadyGenerated => formatter
                .write_str("initial wilderness water generation requires pristine fluid state"),
            Self::LogisticsAlreadyPopulated => formatter
                .write_str("initial wilderness generation requires pristine logistics state"),
            Self::TerrainSeedMismatch {
                established,
                requested,
            } => write!(
                formatter,
                "wilderness seed {} differs from established terrain seed {}",
                requested.value(),
                established.value()
            ),
            Self::SurfaceIdExhausted => {
                formatter.write_str("surface resource identifier space is exhausted")
            }
            Self::SurfaceRevisionExhausted => {
                formatter.write_str("surface resource revision space is exhausted")
            }
            Self::FluidStoreIdExhausted => {
                formatter.write_str("fluid store identifier space is exhausted")
            }
            Self::FluidRevisionExhausted => {
                formatter.write_str("fluid revision space is exhausted")
            }
            Self::LogisticsRevisionExhausted => {
                formatter.write_str("logistics revision space is exhausted")
            }
            Self::NoTraversableTerrainLayout => formatter.write_str(
                "generated terrain has no standing-clearance layout for the bounded wilderness opening",
            ),
            Self::OutcropBoundsOverflow { position } => write!(
                formatter,
                "wilderness outcrop at ({},{},{}) cannot fit a representable one-voxel deposit",
                position.x(),
                position.y(),
                position.z()
            ),
            Self::InvalidGeneratedResource(error) => {
                write!(
                    formatter,
                    "built-in wilderness surface resource is invalid: {error}"
                )
            }
            Self::InvalidGeneratedDeposit(error) => {
                write!(formatter, "built-in wilderness geological seam is invalid: {error}")
            }
            Self::InvalidGeneratedWater => {
                formatter.write_str("built-in wilderness water source is invalid")
            }
        }
    }
}

impl Error for InitialWildernessGenerationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidGeneratedResource(error) => Some(error),
            Self::InvalidGeneratedDeposit(error) => Some(error),
            Self::NotInitialControlTick { .. }
            | Self::NotInitialTick { .. }
            | Self::PlayerAlreadyInitialized { .. }
            | Self::SurfaceAlreadyGenerated
            | Self::GeologyAlreadyGenerated
            | Self::KoboldsAlreadyGenerated
            | Self::FluidAlreadyGenerated
            | Self::LogisticsAlreadyPopulated
            | Self::TerrainSeedMismatch { .. }
            | Self::SurfaceIdExhausted
            | Self::SurfaceRevisionExhausted
            | Self::FluidStoreIdExhausted
            | Self::FluidRevisionExhausted
            | Self::LogisticsRevisionExhausted
            | Self::NoTraversableTerrainLayout
            | Self::OutcropBoundsOverflow { .. }
            | Self::InvalidGeneratedWater => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitialWildernessGenerationCommitError {
    ControlTickChanged {
        expected: ControlTick,
        actual: ControlTick,
    },
    TickChanged {
        expected: SimulationTick,
        actual: SimulationTick,
    },
    PlayerInitialized {
        survival_initialized: bool,
        logistics_initialized: bool,
    },
    StaleSurfaceRevision {
        expected: u64,
        actual: u64,
    },
    StaleGeologyRevision {
        expected: u64,
        actual: u64,
    },
    StaleGeologyIdentity {
        expected: u32,
        actual: u32,
    },
    StaleWorldRevision {
        expected: u64,
        actual: u64,
    },
    StaleStructureRevision {
        expected: u64,
        actual: u64,
    },
    StaleSurfaceIdentity {
        expected: u32,
        actual: u32,
    },
    StaleFluidRevision {
        expected: u64,
        actual: u64,
    },
    StaleFluidIdentity {
        expected: u64,
        actual: u64,
    },
    StaleLogisticsRevision {
        expected: u64,
        actual: u64,
    },
    WaterAlreadyLocated {
        store: FluidStoreId,
    },
}

impl Display for InitialWildernessGenerationCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ControlTickChanged { expected, actual } => write!(
                formatter,
                "initial wilderness generation expected control tick {} but state advanced to {} before commit",
                expected.value(),
                actual.value()
            ),
            Self::TickChanged { expected, actual } => write!(
                formatter,
                "initial wilderness generation expected tick {} but state advanced to {} before commit",
                expected.value(),
                actual.value()
            ),
            Self::PlayerInitialized {
                survival_initialized,
                logistics_initialized,
            } => write!(
                formatter,
                "player admission changed before wilderness commit: survival initialized={survival_initialized}, logistics initialized={logistics_initialized}"
            ),
            Self::StaleSurfaceRevision { expected, actual } => write!(
                formatter,
                "surface revision changed from {expected} to {actual} before wilderness commit"
            ),
            Self::StaleGeologyRevision { expected, actual } => write!(
                formatter,
                "geology revision changed from {expected} to {actual} before wilderness commit"
            ),
            Self::StaleGeologyIdentity { expected, actual } => write!(
                formatter,
                "geological deposit identity changed from {expected} to {actual} before wilderness commit"
            ),
            Self::StaleWorldRevision { expected, actual } => write!(
                formatter,
                "world revision changed from {expected} to {actual} before wilderness commit"
            ),
            Self::StaleStructureRevision { expected, actual } => write!(
                formatter,
                "structure revision changed from {expected} to {actual} before wilderness commit"
            ),
            Self::StaleSurfaceIdentity { expected, actual } => write!(
                formatter,
                "next surface resource id changed from {expected} to {actual} before wilderness commit"
            ),
            Self::StaleFluidRevision { expected, actual } => write!(
                formatter,
                "fluid revision changed from {expected} to {actual} before wilderness commit"
            ),
            Self::StaleFluidIdentity { expected, actual } => write!(
                formatter,
                "next fluid store id changed from {expected} to {actual} before wilderness commit"
            ),
            Self::StaleLogisticsRevision { expected, actual } => write!(
                formatter,
                "logistics revision changed from {expected} to {actual} before wilderness commit"
            ),
            Self::WaterAlreadyLocated { store } => write!(
                formatter,
                "generated water store {} acquired a logistics location before wilderness commit",
                store.value()
            ),
        }
    }
}

impl Error for InitialWildernessGenerationCommitError {}
