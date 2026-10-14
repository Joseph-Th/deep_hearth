//! Deterministic explicit-seed terrain chunk generation.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::state::AppState;
use crate::spatial::VoxelCoord;

use super::{BlockStateId, CHUNK_SIDE, ChunkCoord, WORLD_MIN_Y, WorldChunk, WorldRegistry};

const TERRAIN_LATTICE_SIDE: i64 = 8;
const TERRAIN_HALF_RANGE: i64 = 4;

/// Explicit deterministic seed used only to derive fresh world truth.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WorldGenerationSeed(u64);

impl WorldGenerationSeed {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChunkGenerationError {
    AlreadyGenerated { chunk: ChunkCoord },
    UnknownSolidState { state: BlockStateId },
    ChunkCoordinateOverflow { chunk: ChunkCoord },
    WorldRevisionExhausted,
}

impl Display for ChunkGenerationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
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
            Self::WorldRevisionExhausted => {
                formatter.write_str("terrain generation exhausted world revision capacity")
            }
        }
    }
}

impl Error for ChunkGenerationError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChunkGenerationCommitError {
    StaleWorldRevision { expected: u64, actual: u64 },
    ChunkAppeared { chunk: ChunkCoord },
}

impl Display for ChunkGenerationCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleWorldRevision { expected, actual } => write!(
                formatter,
                "world revision changed from {expected} to {actual} before terrain generation commit"
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

#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedChunkGeneration {
    expected_revision: u64,
    next_revision: u64,
    chunk: WorldChunk,
}

impl ValidatedChunkGeneration {
    #[must_use]
    pub const fn chunk(&self) -> &WorldChunk {
        &self.chunk
    }

    pub fn commit(self, state: &mut AppState) -> Result<ChunkCoord, ChunkGenerationCommitError> {
        let actual = state.world().revision();
        if actual != self.expected_revision {
            return Err(ChunkGenerationCommitError::StaleWorldRevision {
                expected: self.expected_revision,
                actual,
            });
        }
        let coord = self.chunk.coord();
        if state.world().get_chunk(coord).is_some() {
            return Err(ChunkGenerationCommitError::ChunkAppeared { chunk: coord });
        }
        state.world_state_mut().insert_generated_chunk(
            self.expected_revision,
            self.next_revision,
            self.chunk,
        );
        Ok(coord)
    }
}

/// Resolves one deterministic terrain chunk without ambient randomness or external IO.
///
/// `solid_state` is deliberately explicit: geology/biome composition can later select physical
/// strata without hardwiring material policy into compact chunk storage.
pub fn validate_generate_chunk(
    registry: &WorldRegistry,
    state: &AppState,
    seed: WorldGenerationSeed,
    coord: ChunkCoord,
    solid_state: BlockStateId,
) -> Result<ValidatedChunkGeneration, ChunkGenerationError> {
    if state.world().get_chunk(coord).is_some() {
        return Err(ChunkGenerationError::AlreadyGenerated { chunk: coord });
    }
    if registry.get_state(solid_state).is_none() || solid_state == BlockStateId::AIR {
        return Err(ChunkGenerationError::UnknownSolidState { state: solid_state });
    }
    let solid_mass = registry
        .represented_mass_for_state(solid_state)
        .unwrap_or_else(|| unreachable!("validated block state has a block definition"));
    let expected_revision = state.world().revision();
    let next_revision = expected_revision
        .checked_add(1)
        .ok_or(ChunkGenerationError::WorldRevisionExhausted)?;
    let min_x = coord
        .world_min_x()
        .ok_or(ChunkGenerationError::ChunkCoordinateOverflow { chunk: coord })?;
    let min_z = coord
        .world_min_z()
        .ok_or(ChunkGenerationError::ChunkCoordinateOverflow { chunk: coord })?;
    let max_x = min_x
        .checked_add((CHUNK_SIDE - 1) as i64)
        .ok_or(ChunkGenerationError::ChunkCoordinateOverflow { chunk: coord })?;
    let max_z = min_z
        .checked_add((CHUNK_SIDE - 1) as i64)
        .ok_or(ChunkGenerationError::ChunkCoordinateOverflow { chunk: coord })?;
    let _ = (max_x, max_z);
    let mut chunk = WorldChunk::new(coord);
    for local_z in 0..CHUNK_SIDE {
        for local_x in 0..CHUNK_SIDE {
            let x = min_x + local_x as i64;
            let z = min_z + local_z as i64;
            let surface = terrain_height(seed, x, z);
            for y in WORLD_MIN_Y..=surface {
                chunk.set_block(
                    VoxelCoord::new(x, y, z),
                    solid_state,
                    crate::core::quantity::Mass::ZERO,
                    solid_mass,
                );
            }
        }
    }
    chunk.canonicalize_sections();
    Ok(ValidatedChunkGeneration {
        expected_revision,
        next_revision,
        chunk,
    })
}

fn terrain_height(seed: WorldGenerationSeed, x: i64, z: i64) -> i64 {
    let cell_x = x.div_euclid(TERRAIN_LATTICE_SIDE);
    let cell_z = z.div_euclid(TERRAIN_LATTICE_SIDE);
    let fx = x.rem_euclid(TERRAIN_LATTICE_SIDE);
    let fz = z.rem_euclid(TERRAIN_LATTICE_SIDE);
    let h00 = lattice_height(seed, cell_x, cell_z);
    let h10 = lattice_height(seed, cell_x + 1, cell_z);
    let h01 = lattice_height(seed, cell_x, cell_z + 1);
    let h11 = lattice_height(seed, cell_x + 1, cell_z + 1);
    let north = lerp_integer(h00, h10, fx, TERRAIN_LATTICE_SIDE);
    let south = lerp_integer(h01, h11, fx, TERRAIN_LATTICE_SIDE);
    lerp_integer(north, south, fz, TERRAIN_LATTICE_SIDE)
}

fn lattice_height(seed: WorldGenerationSeed, x: i64, z: i64) -> i64 {
    let value = mix64(
        seed.value()
            ^ (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
            ^ (z as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F),
    );
    let width = u64::try_from(TERRAIN_HALF_RANGE * 2 + 1)
        .unwrap_or_else(|_| unreachable!("positive terrain height width fits u64"));
    i64::try_from(value % width).unwrap_or_else(|_| unreachable!("bounded terrain height fits i64"))
        - TERRAIN_HALF_RANGE
}

fn lerp_integer(left: i64, right: i64, offset: i64, span: i64) -> i64 {
    (left * (span - offset) + right * offset).div_euclid(span)
}

fn mix64(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}
