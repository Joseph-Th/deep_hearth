//! Typed player admission, respawn, and revision-conflict errors.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::time::SimulationTick;
use crate::spatial::VoxelCoord;

/// Failure while validating ordinary player admission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerAdmissionError {
    AlreadyAdmitted,
    PartialExistingAdmission {
        survival_initialized: bool,
        logistics_initialized: bool,
    },
    SurvivalRevisionExhausted,
    PositionOutsideContinuousWorld {
        position: VoxelCoord,
    },
    BodyIntersectsSolid {
        voxel: VoxelCoord,
    },
    BodyIntersectsTempleCaste {
        voxel: VoxelCoord,
    },
    BodyIntersectsStructure {
        voxel: VoxelCoord,
    },
    ZeroCarriedCapacity,
    StockpileIdExhausted,
    InventoryRevisionExhausted,
    LogisticsRevisionExhausted,
}

impl Display for PlayerAdmissionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyAdmitted => formatter.write_str("player is already admitted"),
            Self::PartialExistingAdmission {
                survival_initialized,
                logistics_initialized,
            } => write!(
                formatter,
                "player admission is inconsistent: survival initialized={survival_initialized}, logistics initialized={logistics_initialized}"
            ),
            Self::SurvivalRevisionExhausted => {
                formatter.write_str("survival revision space is exhausted")
            }
            Self::PositionOutsideContinuousWorld { position } => write!(
                formatter,
                "player position ({},{},{}) cannot be represented by continuous body coordinates",
                position.x(),
                position.y(),
                position.z()
            ),
            Self::BodyIntersectsSolid { voxel } => write!(
                formatter,
                "player body intersects solid world voxel ({},{},{})",
                voxel.x(),
                voxel.y(),
                voxel.z()
            ),
            Self::BodyIntersectsTempleCaste { voxel } => write!(
                formatter,
                "player body intersects temple resident at ({},{},{})",
                voxel.x(),
                voxel.y(),
                voxel.z()
            ),
            Self::BodyIntersectsStructure { voxel } => write!(
                formatter,
                "player body intersects completed structural matter at ({},{},{})",
                voxel.x(),
                voxel.y(),
                voxel.z()
            ),
            Self::ZeroCarriedCapacity => {
                formatter.write_str("player carried inventory capacity must be nonzero")
            }
            Self::StockpileIdExhausted => {
                formatter.write_str("stockpile identifier space is exhausted")
            }
            Self::InventoryRevisionExhausted => {
                formatter.write_str("inventory revision space is exhausted")
            }
            Self::LogisticsRevisionExhausted => {
                formatter.write_str("logistics revision space is exhausted")
            }
        }
    }
}

impl Error for PlayerAdmissionError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerRespawnError {
    PlayerNotAdmitted,
    PlayerAlive,
    ActivePlayerWork,
    PendingDirectAction,
    PendingDirectConsumption,
    NoSafeRespawnLocation,
    SurvivalRevisionExhausted,
    StockpileIdExhausted,
    InventoryRevisionExhausted,
    LogisticsRevisionExhausted,
}

impl Display for PlayerRespawnError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlayerNotAdmitted => formatter.write_str("player is not admitted"),
            Self::PlayerAlive => formatter.write_str("living player cannot respawn"),
            Self::ActivePlayerWork => {
                formatter.write_str("player cannot respawn while work is active")
            }
            Self::PendingDirectAction => {
                formatter.write_str("player cannot respawn with buffered direct input")
            }
            Self::PendingDirectConsumption => {
                formatter.write_str("player cannot respawn with pending direct consumption")
            }
            Self::NoSafeRespawnLocation => {
                formatter.write_str("no collision-free, supported player respawn location exists")
            }
            Self::SurvivalRevisionExhausted => {
                formatter.write_str("survival revision space is exhausted")
            }
            Self::StockpileIdExhausted => {
                formatter.write_str("stockpile identifier space is exhausted")
            }
            Self::InventoryRevisionExhausted => {
                formatter.write_str("inventory revision space is exhausted")
            }
            Self::LogisticsRevisionExhausted => {
                formatter.write_str("logistics revision space is exhausted")
            }
        }
    }
}

impl Error for PlayerRespawnError {}

/// Failure when validated player admission is committed after one of its owners changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerAdmissionCommitError {
    StaleSimulationTick {
        expected: SimulationTick,
        actual: SimulationTick,
    },
    StaleSurvivalRevision {
        expected: u64,
        actual: u64,
    },
    StaleLogisticsRevision {
        expected: u64,
        actual: u64,
    },
    StaleInventoryRevision {
        expected: u64,
        actual: u64,
    },
    StaleWorldRevision {
        expected: u64,
        actual: u64,
    },
    StaleStructureRevision {
        expected: u64,
        actual: u64,
    },
}

impl Display for PlayerAdmissionCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleSimulationTick { expected, actual } => write!(
                formatter,
                "validated admission expected simulation tick {} but current tick is {}",
                expected.value(),
                actual.value()
            ),
            Self::StaleSurvivalRevision { expected, actual } => write!(
                formatter,
                "validated player admission expected survival revision {expected} but current revision is {actual}"
            ),
            Self::StaleLogisticsRevision { expected, actual } => write!(
                formatter,
                "validated player admission expected logistics revision {expected} but current revision is {actual}"
            ),
            Self::StaleInventoryRevision { expected, actual } => write!(
                formatter,
                "validated player admission expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::StaleWorldRevision { expected, actual } => write!(
                formatter,
                "validated player admission expected world revision {expected} but current revision is {actual}"
            ),
            Self::StaleStructureRevision { expected, actual } => write!(
                formatter,
                "validated player admission expected structure revision {expected} but current revision is {actual}"
            ),
        }
    }
}

impl Error for PlayerAdmissionCommitError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerRespawnCommitError {
    StaleSimulationTick {
        expected: SimulationTick,
        actual: SimulationTick,
    },
    StaleSurvivalRevision {
        expected: u64,
        actual: u64,
    },
    StaleLogisticsRevision {
        expected: u64,
        actual: u64,
    },
    StaleInventoryRevision {
        expected: u64,
        actual: u64,
    },
    StaleWorldRevision {
        expected: u64,
        actual: u64,
    },
    StaleStructureRevision {
        expected: u64,
        actual: u64,
    },
}

impl Display for PlayerRespawnCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleSimulationTick { expected, actual } => write!(
                formatter,
                "validated respawn expected simulation tick {} but current tick is {}",
                expected.value(),
                actual.value()
            ),
            Self::StaleSurvivalRevision { expected, actual } => write!(
                formatter,
                "validated respawn expected survival revision {expected} but current revision is {actual}"
            ),
            Self::StaleLogisticsRevision { expected, actual } => write!(
                formatter,
                "validated respawn expected logistics revision {expected} but current revision is {actual}"
            ),
            Self::StaleInventoryRevision { expected, actual } => write!(
                formatter,
                "validated respawn expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::StaleWorldRevision { expected, actual } => write!(
                formatter,
                "validated respawn expected world revision {expected} but current revision is {actual}"
            ),
            Self::StaleStructureRevision { expected, actual } => write!(
                formatter,
                "validated respawn expected structure revision {expected} but current revision is {actual}"
            ),
        }
    }
}

impl Error for PlayerRespawnCommitError {}
