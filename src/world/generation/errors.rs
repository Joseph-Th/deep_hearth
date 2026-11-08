//! Typed terrain admission and optimistic-commit failures.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::time::SimulationTick;
use crate::spatial::VoxelCoord;
use crate::world::{BlockStateId, ChunkCoord};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChunkGenerationError {
    PlayerNotAdmitted,
    GenerationProfileMissing,
    GenerationProfileConflict,
    AlreadyGenerated { chunk: ChunkCoord },
    UnknownSolidState { state: BlockStateId },
    ChunkCoordinateOverflow { chunk: ChunkCoord },
    PlayerBodyCollision { voxel: VoxelCoord },
    ActivePlayerWorkAtGenerationSite { chunk: ChunkCoord },
    OpeningResidentBodyCollision { voxel: VoxelCoord },
    OpeningResidentTempleCasteCollision { voxel: VoxelCoord },
    PlayerBodyTempleCasteCollision { voxel: VoxelCoord },
    TempleCasteInfrastructureCollision { voxel: VoxelCoord },
    StructureBuriedByTerrain { voxel: VoxelCoord },
    StorageEnclosureBuriedByTerrain { voxel: VoxelCoord },
    GroundMatterBuriedByTerrain { voxel: VoxelCoord },
    FluidStoreBuriedByTerrain { voxel: VoxelCoord },
    EnergyStoreBuriedByTerrain { voxel: VoxelCoord },
    EquipmentBuriedByTerrain { voxel: VoxelCoord },
    SurfaceResourceCollision { voxel: VoxelCoord },
    TreePlacementObstructed { voxel: VoxelCoord },
    PlayerDemandCoordinateOverflow { center: ChunkCoord },
    WorldRevisionExhausted,
    ForageIdExhausted,
    ForageRevisionExhausted,
    InvalidForageResource(crate::surface::SurfaceResourceValidationError),
    SpringIdExhausted,
    SpringRevisionExhausted,
    InvalidSpringWater,
    LogisticsRevisionExhausted,
}

impl Display for ChunkGenerationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlayerNotAdmitted => {
                formatter.write_str("terrain chunk demand requires an admitted player")
            }
            Self::GenerationProfileMissing => formatter.write_str(
                "terrain chunk demand requires an initialized persistent generation profile",
            ),
            Self::GenerationProfileConflict => formatter.write_str(
                "terrain generation request conflicts with the world's established generation identity",
            ),
            Self::AlreadyGenerated { chunk } => write!(
                formatter,
                "world chunk ({},{}) is already generated",
                chunk.x(),
                chunk.z()
            ),
            Self::UnknownSolidState { state } => write!(
                formatter,
                "terrain generation requested unknown block state {}",
                state.value()
            ),
            Self::ChunkCoordinateOverflow { chunk } => write!(
                formatter,
                "world chunk ({},{}) lies outside the representable voxel-coordinate range",
                chunk.x(),
                chunk.z()
            ),
            Self::PlayerBodyCollision { voxel } => write!(
                formatter,
                "terrain generation would intersect the player body at voxel ({},{},{})",
                voxel.x(),
                voxel.y(),
                voxel.z()
            ),
            Self::ActivePlayerWorkAtGenerationSite { chunk } => write!(
                formatter,
                "cannot generate chunk ({},{}) through the footprint of a player performing stationary work",
                chunk.x(),
                chunk.z()
            ),
            Self::OpeningResidentBodyCollision { voxel } => write!(
                formatter,
                "terrain generation would bury an opening kobold resident at voxel ({},{},{})",
                voxel.x(),
                voxel.y(),
                voxel.z()
            ),
            Self::OpeningResidentTempleCasteCollision { voxel } => write!(
                formatter,
                "terrain generation would overlap an opening kobold with a temple resident at voxel ({},{},{})",
                voxel.x(),
                voxel.y(),
                voxel.z()
            ),
            Self::PlayerBodyTempleCasteCollision { voxel } => write!(
                formatter,
                "terrain generation would instantiate a temple resident inside the player at voxel ({},{},{})",
                voxel.x(),
                voxel.y(),
                voxel.z()
            ),
            Self::TempleCasteInfrastructureCollision { voxel } => write!(
                formatter,
                "terrain generation would instantiate a temple resident inside constructed or stationary custody at voxel ({},{},{})",
                voxel.x(),
                voxel.y(),
                voxel.z()
            ),
            Self::StructureBuriedByTerrain { voxel } => write!(
                formatter,
                "terrain generation would bury embodied structural matter at voxel ({},{},{})",
                voxel.x(),
                voxel.y(),
                voxel.z()
            ),
            Self::StorageEnclosureBuriedByTerrain { voxel } => write!(
                formatter,
                "terrain generation would bury a completed storage enclosure at voxel ({},{},{})",
                voxel.x(),
                voxel.y(),
                voxel.z()
            ),
            Self::GroundMatterBuriedByTerrain { voxel } => write!(
                formatter,
                "terrain generation would bury stored or reserved loose ground matter at voxel ({},{},{})",
                voxel.x(),
                voxel.y(),
                voxel.z()
            ),
            Self::FluidStoreBuriedByTerrain { voxel } => write!(
                formatter,
                "terrain generation would bury a located fluid vessel at voxel ({},{},{})",
                voxel.x(),
                voxel.y(),
                voxel.z()
            ),
            Self::EnergyStoreBuriedByTerrain { voxel } => write!(
                formatter,
                "terrain generation would bury a material-built energy store at voxel ({},{},{})",
                voxel.x(),
                voxel.y(),
                voxel.z()
            ),
            Self::EquipmentBuriedByTerrain { voxel } => write!(
                formatter,
                "terrain generation would bury stationary equipment at voxel ({},{},{})",
                voxel.x(),
                voxel.y(),
                voxel.z()
            ),
            Self::SurfaceResourceCollision { voxel } => write!(
                formatter,
                "terrain generation would bury exposed surface matter at voxel ({},{},{})",
                voxel.x(),
                voxel.y(),
                voxel.z()
            ),
            Self::TreePlacementObstructed { voxel } => write!(
                formatter,
                "generated tree trunk at ({},{},{}) would intersect an existing actor or material",
                voxel.x(), voxel.y(), voxel.z()
            ),
            Self::PlayerDemandCoordinateOverflow { center } => write!(
                formatter,
                "player chunk demand around ({},{}) exceeds the supported chunk-coordinate range",
                center.x(),
                center.z()
            ),
            Self::WorldRevisionExhausted => {
                formatter.write_str("terrain generation exhausted world revision capacity")
            }
            Self::ForageIdExhausted => formatter.write_str("terrain forage exhausted resource identifiers"),
            Self::ForageRevisionExhausted => formatter.write_str("terrain forage exhausted surface revisions"),
            Self::InvalidForageResource(error) => write!(formatter, "terrain forage is invalid: {error}"),
            Self::SpringIdExhausted => formatter.write_str("natural water exhausted store identifiers"),
            Self::SpringRevisionExhausted => formatter.write_str("natural water exhausted fluid revisions"),
            Self::InvalidSpringWater => formatter.write_str("natural water has invalid authored fluid properties"),
            Self::LogisticsRevisionExhausted => formatter.write_str("natural spring generation exhausted logistics revisions"),
        }
    }
}

impl Error for ChunkGenerationError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChunkGenerationCommitError {
    SpringClockChanged {
        expected: SimulationTick,
        actual: SimulationTick,
    },
    StaleFluidRevision {
        expected: u64,
        actual: u64,
    },
    StaleFluidIdentity {
        expected: u64,
        actual: u64,
    },
    StaleWorldRevision {
        expected: u64,
        actual: u64,
    },
    StaleSurfaceRevision {
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
    StaleStructureRevision {
        expected: u64,
        actual: u64,
    },
    StalePlayerWorkRevision {
        expected: u64,
        actual: u64,
    },
    StaleOpeningResidents {
        expected: Option<[VoxelCoord; 2]>,
        actual: Option<[VoxelCoord; 2]>,
    },
    ChunkAppeared {
        chunk: ChunkCoord,
    },
}

impl Display for ChunkGenerationCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SpringClockChanged { expected, actual } => write!(
                formatter,
                "natural-water plan was built at tick {} but the current tick is {}",
                expected.value(),
                actual.value()
            ),
            Self::StaleFluidRevision { expected, actual } => write!(
                formatter,
                "fluid revision changed from {expected} to {actual} before natural spring generation"
            ),
            Self::StaleFluidIdentity { expected, actual } => write!(
                formatter,
                "fluid next-store identity changed from {expected} to {actual} before natural spring generation"
            ),
            Self::StaleWorldRevision { expected, actual } => write!(
                formatter,
                "world revision changed from {expected} to {actual} before terrain generation commit"
            ),
            Self::StaleSurfaceRevision { expected, actual } => write!(
                formatter,
                "surface revision changed from {expected} to {actual} before terrain generation commit"
            ),
            Self::StaleLogisticsRevision { expected, actual } => write!(
                formatter,
                "logistics revision changed from {expected} to {actual} before terrain generation commit"
            ),
            Self::StaleInventoryRevision { expected, actual } => write!(
                formatter,
                "inventory revision changed from {expected} to {actual} before terrain generation commit"
            ),
            Self::StaleStructureRevision { expected, actual } => write!(
                formatter,
                "structure revision changed from {expected} to {actual} before terrain generation commit"
            ),
            Self::StalePlayerWorkRevision { expected, actual } => write!(
                formatter,
                "player-work revision changed from {expected} to {actual} before terrain generation commit"
            ),
            Self::StaleOpeningResidents { expected, actual } => write!(
                formatter,
                "opening resident positions changed from {expected:?} to {actual:?} before terrain generation commit"
            ),
            Self::ChunkAppeared { chunk } => write!(
                formatter,
                "world chunk ({},{}) was generated before this terrain commit",
                chunk.x(),
                chunk.z()
            ),
        }
    }
}

impl Error for ChunkGenerationCommitError {}
