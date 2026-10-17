//! Deterministic terrain identity, explicit chunk generation, and player-neighborhood demand.

use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::core::state::AppState;
use crate::logistics::first_player_body_collision_in_chunk;
use crate::player::{PlayerAdmissionState, player_admission_state};
use crate::registry::Registries;
use crate::spatial::VoxelCoord;

use super::{BlockStateId, CHUNK_SIDE, ChunkCoord, WORLD_MIN_Y, WorldChunk, WorldRegistry};

const TERRAIN_LATTICE_SIDE: i64 = 8;
const TERRAIN_HALF_RANGE: i64 = 4;
const PLAYER_CHUNK_DEMAND_RADIUS: i64 = 1;

/// Deterministic seed that identifies one generated terrain world.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
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

/// Persisted identity of the deterministic terrain generator used by one world.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorldGenerationProfile {
    seed: WorldGenerationSeed,
    solid_state: BlockStateId,
}

impl WorldGenerationProfile {
    #[must_use]
    pub(crate) const fn new(seed: WorldGenerationSeed, solid_state: BlockStateId) -> Self {
        Self { seed, solid_state }
    }

    #[must_use]
    pub(crate) const fn seed(self) -> WorldGenerationSeed {
        self.seed
    }

    #[must_use]
    pub(crate) const fn solid_state(self) -> BlockStateId {
        self.solid_state
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChunkGenerationError {
    PlayerNotAdmitted,
    GenerationProfileMissing,
    GenerationProfileConflict,
    AlreadyGenerated { chunk: ChunkCoord },
    UnknownSolidState { state: BlockStateId },
    ChunkCoordinateOverflow { chunk: ChunkCoord },
    PlayerBodyCollision { voxel: VoxelCoord },
    PlayerDemandCoordinateOverflow { center: ChunkCoord },
    WorldRevisionExhausted,
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
            Self::PlayerDemandCoordinateOverflow { center } => write!(
                formatter,
                "player chunk demand around ({},{}) exceeds the supported chunk-coordinate range",
                center.x(),
                center.z()
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
    StaleLogisticsRevision { expected: u64, actual: u64 },
    StalePlayerWorkRevision { expected: u64, actual: u64 },
    ChunkAppeared { chunk: ChunkCoord },
}

impl Display for ChunkGenerationCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleWorldRevision { expected, actual } => write!(
                formatter,
                "world revision changed from {expected} to {actual} before terrain generation commit"
            ),
            Self::StaleLogisticsRevision { expected, actual } => write!(
                formatter,
                "logistics revision changed from {expected} to {actual} before terrain generation commit"
            ),
            Self::StalePlayerWorkRevision { expected, actual } => write!(
                formatter,
                "player-work revision changed from {expected} to {actual} before terrain generation commit"
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
    expected_logistics_revision: u64,
    expected_player_work_revision: u64,
    next_revision: u64,
    profile: WorldGenerationProfile,
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
        let actual_logistics_revision = state.logistics().revision();
        if actual_logistics_revision != self.expected_logistics_revision {
            return Err(ChunkGenerationCommitError::StaleLogisticsRevision {
                expected: self.expected_logistics_revision,
                actual: actual_logistics_revision,
            });
        }
        let actual_player_work_revision = state.player_work().revision();
        if actual_player_work_revision != self.expected_player_work_revision {
            return Err(ChunkGenerationCommitError::StalePlayerWorkRevision {
                expected: self.expected_player_work_revision,
                actual: actual_player_work_revision,
            });
        }
        let coord = self.chunk.coord();
        if state.world().get_chunk(coord).is_some() {
            return Err(ChunkGenerationCommitError::ChunkAppeared { chunk: coord });
        }
        state.world_state_mut().insert_generated_chunks(
            self.expected_revision,
            self.next_revision,
            self.profile,
            vec![self.chunk],
        );
        Ok(coord)
    }
}

/// One atomic generated-neighborhood expansion around the admitted player.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedPlayerChunkDemand {
    expected_revision: u64,
    expected_logistics_revision: u64,
    expected_player_work_revision: u64,
    next_revision: u64,
    profile: WorldGenerationProfile,
    chunks: Vec<WorldChunk>,
}

impl ValidatedPlayerChunkDemand {
    pub fn chunks(&self) -> impl ExactSizeIterator<Item = &WorldChunk> {
        self.chunks.iter()
    }

    pub fn commit(
        self,
        state: &mut AppState,
    ) -> Result<Vec<ChunkCoord>, ChunkGenerationCommitError> {
        let actual = state.world().revision();
        if actual != self.expected_revision {
            return Err(ChunkGenerationCommitError::StaleWorldRevision {
                expected: self.expected_revision,
                actual,
            });
        }
        let actual_logistics_revision = state.logistics().revision();
        if actual_logistics_revision != self.expected_logistics_revision {
            return Err(ChunkGenerationCommitError::StaleLogisticsRevision {
                expected: self.expected_logistics_revision,
                actual: actual_logistics_revision,
            });
        }
        let actual_player_work_revision = state.player_work().revision();
        if actual_player_work_revision != self.expected_player_work_revision {
            return Err(ChunkGenerationCommitError::StalePlayerWorkRevision {
                expected: self.expected_player_work_revision,
                actual: actual_player_work_revision,
            });
        }
        for chunk in &self.chunks {
            if state.world().get_chunk(chunk.coord()).is_some() {
                return Err(ChunkGenerationCommitError::ChunkAppeared {
                    chunk: chunk.coord(),
                });
            }
        }
        let coords = self
            .chunks
            .iter()
            .map(WorldChunk::coord)
            .collect::<Vec<_>>();
        state.world_state_mut().insert_generated_chunks(
            self.expected_revision,
            self.next_revision,
            self.profile,
            self.chunks,
        );
        Ok(coords)
    }
}

/// Resolves one deterministic terrain chunk without ambient randomness or external IO.
///
/// `solid_state` is deliberately explicit: geology/biome composition can later select physical
/// strata without hardwiring material policy into compact chunk storage.
pub fn validate_generate_chunk(
    registries: &Registries,
    state: &AppState,
    seed: WorldGenerationSeed,
    coord: ChunkCoord,
    solid_state: BlockStateId,
) -> Result<ValidatedChunkGeneration, ChunkGenerationError> {
    let registry = registries.world();
    if state.world().get_chunk(coord).is_some() {
        return Err(ChunkGenerationError::AlreadyGenerated { chunk: coord });
    }
    let profile = WorldGenerationProfile::new(seed, solid_state);
    validate_generation_profile(registry, state, profile)?;
    let expected_revision = state.world().revision();
    let next_revision = expected_revision
        .checked_add(1)
        .ok_or(ChunkGenerationError::WorldRevisionExhausted)?;
    let chunk = build_generated_chunk(registry, profile, coord)?;
    if let Some(player) = state.logistics().player()
        && let Some(voxel) = first_player_body_collision_in_chunk(
            registry,
            &chunk,
            player.body(),
            registries.survival().physiology().locomotion(),
        )
    {
        return Err(ChunkGenerationError::PlayerBodyCollision { voxel });
    }
    Ok(ValidatedChunkGeneration {
        expected_revision,
        expected_logistics_revision: state.logistics().revision(),
        expected_player_work_revision: state.player_work().revision(),
        next_revision,
        profile,
        chunk,
    })
}

/// Resolves the missing 3x3 chunk neighborhood around the admitted player as one world mutation.
///
/// The world must already have one explicit generation profile established by ordinary chunk
/// generation. This operation then reuses that persisted identity, so ongoing access cannot
/// silently switch seed or material policy and callers do not need to retain either value.
pub fn validate_generate_player_neighborhood(
    registries: &Registries,
    state: &AppState,
) -> Result<Option<ValidatedPlayerChunkDemand>, ChunkGenerationError> {
    if player_admission_state(state) != PlayerAdmissionState::Admitted {
        return Err(ChunkGenerationError::PlayerNotAdmitted);
    }
    let player = state
        .logistics()
        .player()
        .copied()
        .unwrap_or_else(|| unreachable!("admitted player has logistics state"));
    let profile = state
        .world()
        .generation_profile()
        .ok_or(ChunkGenerationError::GenerationProfileMissing)?;
    validate_generation_profile(registries.world(), state, profile)?;
    let center = ChunkCoord::containing(player.position());
    let mut chunks = Vec::new();
    for x_offset in -PLAYER_CHUNK_DEMAND_RADIUS..=PLAYER_CHUNK_DEMAND_RADIUS {
        for z_offset in -PLAYER_CHUNK_DEMAND_RADIUS..=PLAYER_CHUNK_DEMAND_RADIUS {
            let x = center
                .x()
                .checked_add(x_offset)
                .ok_or(ChunkGenerationError::PlayerDemandCoordinateOverflow { center })?;
            let z = center
                .z()
                .checked_add(z_offset)
                .ok_or(ChunkGenerationError::PlayerDemandCoordinateOverflow { center })?;
            let coord = ChunkCoord::new(x, z);
            if state.world().get_chunk(coord).is_some() {
                continue;
            }
            let chunk = build_generated_chunk(registries.world(), profile, coord)?;
            if let Some(voxel) = first_player_body_collision_in_chunk(
                registries.world(),
                &chunk,
                player.body(),
                registries.survival().physiology().locomotion(),
            ) {
                return Err(ChunkGenerationError::PlayerBodyCollision { voxel });
            }
            chunks.push(chunk);
        }
    }
    if chunks.is_empty() {
        return Ok(None);
    }
    chunks.sort_unstable_by_key(WorldChunk::coord);
    let expected_revision = state.world().revision();
    let next_revision = expected_revision
        .checked_add(1)
        .ok_or(ChunkGenerationError::WorldRevisionExhausted)?;
    Ok(Some(ValidatedPlayerChunkDemand {
        expected_revision,
        expected_logistics_revision: state.logistics().revision(),
        expected_player_work_revision: state.player_work().revision(),
        next_revision,
        profile,
        chunks,
    }))
}

fn validate_generation_profile(
    registry: &WorldRegistry,
    state: &AppState,
    requested: WorldGenerationProfile,
) -> Result<(), ChunkGenerationError> {
    if registry.get_state(requested.solid_state()).is_none()
        || requested.solid_state() == BlockStateId::AIR
    {
        return Err(ChunkGenerationError::UnknownSolidState {
            state: requested.solid_state(),
        });
    }
    if let Some(existing) = state.world().generation_profile()
        && existing != requested
    {
        return Err(ChunkGenerationError::GenerationProfileConflict);
    }
    Ok(())
}

fn build_generated_chunk(
    registry: &WorldRegistry,
    profile: WorldGenerationProfile,
    coord: ChunkCoord,
) -> Result<WorldChunk, ChunkGenerationError> {
    let min_x = coord
        .world_min_x()
        .ok_or(ChunkGenerationError::ChunkCoordinateOverflow { chunk: coord })?;
    let min_z = coord
        .world_min_z()
        .ok_or(ChunkGenerationError::ChunkCoordinateOverflow { chunk: coord })?;
    min_x
        .checked_add((CHUNK_SIDE - 1) as i64)
        .ok_or(ChunkGenerationError::ChunkCoordinateOverflow { chunk: coord })?;
    min_z
        .checked_add((CHUNK_SIDE - 1) as i64)
        .ok_or(ChunkGenerationError::ChunkCoordinateOverflow { chunk: coord })?;
    let solid_mass = registry
        .represented_mass_for_state(profile.solid_state())
        .unwrap_or_else(|| unreachable!("validated generation profile has a block definition"));
    let mut chunk = WorldChunk::new(coord);
    for local_z in 0..CHUNK_SIDE {
        for local_x in 0..CHUNK_SIDE {
            let x = min_x + local_x as i64;
            let z = min_z + local_z as i64;
            let surface = terrain_height(profile.seed(), x, z);
            for y in WORLD_MIN_Y..=surface {
                chunk.set_block(
                    VoxelCoord::new(x, y, z),
                    profile.solid_state(),
                    crate::core::quantity::Mass::ZERO,
                    solid_mass,
                );
            }
        }
    }
    chunk.canonicalize_sections();
    Ok(chunk)
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
