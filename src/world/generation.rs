//! Deterministic terrain identity, explicit chunk generation, and player-neighborhood demand.

pub(super) mod temple;

use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::core::state::AppState;
use crate::logistics::first_player_body_collision_in_chunk;
use crate::player::{PlayerAdmissionState, player_admission_state};
use crate::registry::Registries;
use crate::spatial::{VoxelBounds, VoxelCoord};

use super::{
    BlockCollisionShape, BlockStateId, CHUNK_SIDE, ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y,
    WorldChunk, WorldRegistry,
};

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
    OpeningResidentBodyCollision { voxel: VoxelCoord },
    OpeningResidentTempleCasteCollision { voxel: VoxelCoord },
    PlayerBodyTempleCasteCollision { voxel: VoxelCoord },
    TempleCasteInfrastructureCollision { voxel: VoxelCoord },
    StructureBuriedByTerrain { voxel: VoxelCoord },
    StorageEnclosureBuriedByTerrain { voxel: VoxelCoord },
    SurfaceResourceCollision { voxel: VoxelCoord },
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
            Self::SurfaceResourceCollision { voxel } => write!(
                formatter,
                "terrain generation would bury exposed surface matter at voxel ({},{},{})",
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

/// Geometry admission depends on these live actor positions, which have no
/// standalone revision. Recheck the physical snapshot only when old or new
/// resident occupancy intersects an incoming chunk. Distant patrol steps
/// cannot change this plan's legality and must not stall unrelated streaming.
fn validate_unchanged_opening_residents(
    state: &AppState,
    expected: Option<[VoxelCoord; 2]>,
    chunks: &[WorldChunk],
) -> Result<(), ChunkGenerationCommitError> {
    let actual = crate::kobolds::opening_kobold_positions(state);
    let affects_chunks = [expected, actual]
        .into_iter()
        .flatten()
        .flatten()
        .any(|position| {
            chunks
                .iter()
                .any(|chunk| chunk.coord() == ChunkCoord::containing(position))
        });
    if actual != expected && affects_chunks {
        return Err(ChunkGenerationCommitError::StaleOpeningResidents { expected, actual });
    }
    Ok(())
}

#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedChunkGeneration {
    expected_revision: u64,
    expected_surface_revision: u64,
    expected_logistics_revision: u64,
    expected_inventory_revision: u64,
    expected_structure_revision: u64,
    expected_player_work_revision: u64,
    expected_opening_residents: Option<[VoxelCoord; 2]>,
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
        let actual_surface_revision = state.surface().revision();
        if actual_surface_revision != self.expected_surface_revision {
            return Err(ChunkGenerationCommitError::StaleSurfaceRevision {
                expected: self.expected_surface_revision,
                actual: actual_surface_revision,
            });
        }
        let actual_logistics_revision = state.logistics().revision();
        if actual_logistics_revision != self.expected_logistics_revision {
            return Err(ChunkGenerationCommitError::StaleLogisticsRevision {
                expected: self.expected_logistics_revision,
                actual: actual_logistics_revision,
            });
        }
        let actual_structure_revision = state.structures().revision();
        if actual_structure_revision != self.expected_structure_revision {
            return Err(ChunkGenerationCommitError::StaleStructureRevision {
                expected: self.expected_structure_revision,
                actual: actual_structure_revision,
            });
        }
        let actual_player_work_revision = state.player_work().revision();
        if actual_player_work_revision != self.expected_player_work_revision {
            return Err(ChunkGenerationCommitError::StalePlayerWorkRevision {
                expected: self.expected_player_work_revision,
                actual: actual_player_work_revision,
            });
        }
        let actual_inventory_revision = state.inventory().revision();
        if actual_inventory_revision != self.expected_inventory_revision {
            return Err(ChunkGenerationCommitError::StaleInventoryRevision {
                expected: self.expected_inventory_revision,
                actual: actual_inventory_revision,
            });
        }
        let coord = self.chunk.coord();
        validate_unchanged_opening_residents(
            state,
            self.expected_opening_residents,
            std::slice::from_ref(&self.chunk),
        )?;
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
    expected_surface_revision: u64,
    expected_logistics_revision: u64,
    expected_inventory_revision: u64,
    expected_structure_revision: u64,
    expected_player_work_revision: u64,
    expected_opening_residents: Option<[VoxelCoord; 2]>,
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
        let actual_surface_revision = state.surface().revision();
        if actual_surface_revision != self.expected_surface_revision {
            return Err(ChunkGenerationCommitError::StaleSurfaceRevision {
                expected: self.expected_surface_revision,
                actual: actual_surface_revision,
            });
        }
        let actual_logistics_revision = state.logistics().revision();
        if actual_logistics_revision != self.expected_logistics_revision {
            return Err(ChunkGenerationCommitError::StaleLogisticsRevision {
                expected: self.expected_logistics_revision,
                actual: actual_logistics_revision,
            });
        }
        let actual_structure_revision = state.structures().revision();
        if actual_structure_revision != self.expected_structure_revision {
            return Err(ChunkGenerationCommitError::StaleStructureRevision {
                expected: self.expected_structure_revision,
                actual: actual_structure_revision,
            });
        }
        let actual_player_work_revision = state.player_work().revision();
        if actual_player_work_revision != self.expected_player_work_revision {
            return Err(ChunkGenerationCommitError::StalePlayerWorkRevision {
                expected: self.expected_player_work_revision,
                actual: actual_player_work_revision,
            });
        }
        let actual_inventory_revision = state.inventory().revision();
        if actual_inventory_revision != self.expected_inventory_revision {
            return Err(ChunkGenerationCommitError::StaleInventoryRevision {
                expected: self.expected_inventory_revision,
                actual: actual_inventory_revision,
            });
        }
        validate_unchanged_opening_residents(state, self.expected_opening_residents, &self.chunks)?;
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
    if !state.can_spend_world_revisions(1) {
        return Err(ChunkGenerationError::WorldRevisionExhausted);
    }
    let expected_revision = state.world().revision();
    let next_revision = expected_revision
        .checked_add(1)
        .unwrap_or_else(|| unreachable!("world headroom check includes chunk generation"));
    let chunk = build_generated_chunk(registry, profile, coord)?;
    validate_generated_chunk_occupancy(registries, state, profile, &chunk)?;
    Ok(ValidatedChunkGeneration {
        expected_revision,
        expected_surface_revision: state.surface().revision(),
        expected_logistics_revision: state.logistics().revision(),
        expected_inventory_revision: state.inventory().revision(),
        expected_structure_revision: state.structures().revision(),
        expected_player_work_revision: state.player_work().revision(),
        expected_opening_residents: crate::kobolds::opening_kobold_positions(state),
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
            validate_generated_chunk_occupancy(registries, state, profile, &chunk)?;
            chunks.push(chunk);
        }
    }
    if chunks.is_empty() {
        return Ok(None);
    }
    chunks.sort_unstable_by_key(WorldChunk::coord);
    if !state.can_spend_world_revisions(1) {
        return Err(ChunkGenerationError::WorldRevisionExhausted);
    }
    let expected_revision = state.world().revision();
    let next_revision = expected_revision
        .checked_add(1)
        .unwrap_or_else(|| unreachable!("world headroom check includes player chunk demand"));
    Ok(Some(ValidatedPlayerChunkDemand {
        expected_revision,
        expected_surface_revision: state.surface().revision(),
        expected_logistics_revision: state.logistics().revision(),
        expected_inventory_revision: state.inventory().revision(),
        expected_structure_revision: state.structures().revision(),
        expected_player_work_revision: state.player_work().revision(),
        expected_opening_residents: crate::kobolds::opening_kobold_positions(state),
        next_revision,
        profile,
        chunks,
    }))
}

/// One shared physical admission for both direct and atomic neighborhood
/// generation; future occupied-matter categories must be checked in both.
fn validate_generated_chunk_occupancy(
    registries: &Registries,
    state: &AppState,
    profile: WorldGenerationProfile,
    chunk: &WorldChunk,
) -> Result<(), ChunkGenerationError> {
    let registry = registries.world();
    if let Some(voxel) = first_surface_resource_collision(registry, state, chunk) {
        return Err(ChunkGenerationError::SurfaceResourceCollision { voxel });
    }
    if let Some(voxel) = first_structure_collision_in_generated_chunk(registry, state, chunk) {
        return Err(ChunkGenerationError::StructureBuriedByTerrain { voxel });
    }
    if let Some(voxel) =
        first_stationary_enclosure_collision_in_generated_chunk(registry, state, chunk)
    {
        return Err(ChunkGenerationError::StorageEnclosureBuriedByTerrain { voxel });
    }
    if let Some(player) = state.logistics().player()
        && let Some(voxel) = first_player_body_collision_in_chunk(
            registry,
            chunk,
            player.body(),
            registries.survival().physiology().locomotion(),
        )
    {
        return Err(ChunkGenerationError::PlayerBodyCollision { voxel });
    }
    if let Some(voxel) = first_opening_resident_collision_in_chunk(registry, state, chunk) {
        return Err(ChunkGenerationError::OpeningResidentBodyCollision { voxel });
    }
    let coord = chunk.coord();
    if let Some(voxel) = first_prospective_temple_overlap(registry, profile, coord, |cell| {
        crate::kobolds::opening_kobold_occupies_voxel(state, cell)
    }) {
        return Err(ChunkGenerationError::OpeningResidentTempleCasteCollision { voxel });
    }
    if let Some(voxel) =
        first_player_prospective_temple_collision(registries, state, profile, coord)
    {
        return Err(ChunkGenerationError::PlayerBodyTempleCasteCollision { voxel });
    }
    if let Some(voxel) =
        first_infrastructure_prospective_temple_collision(registries, state, profile, coord)
    {
        return Err(ChunkGenerationError::TempleCasteInfrastructureCollision { voxel });
    }
    Ok(())
}

/// A live opening colony can inhabit authored route cells before chunk
/// generation. Do not create full-cube terrain through the scout or the
/// watchman's two-cell body; unoccupied future route cells remain subject
/// to the ordinary movement clearance gate.
fn first_opening_resident_collision_in_chunk(
    registry: &WorldRegistry,
    state: &AppState,
    chunk: &WorldChunk,
) -> Option<VoxelCoord> {
    let residents = crate::kobolds::opening_kobold_positions(state)?;
    for resident in residents {
        for y in [Some(resident.y()), resident.y().checked_add(1)]
            .into_iter()
            .flatten()
        {
            let cell = VoxelCoord::new(resident.x(), y, resident.z());
            if ChunkCoord::containing(cell) == chunk.coord()
                && (!(WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE).contains(&cell.y())
                    || registry
                        .block_for_state(chunk.block_at(cell))
                        .is_some_and(|block| block.collision() == BlockCollisionShape::FullCube))
            {
                return Some(cell);
            }
        }
    }
    None
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
    // Alternate authored registries used by capability scenarios may not have ancient
    // relic blocks. Such worlds retain their valid original terrain identity.
    let temple = temple_site_for_chunk(registry, profile, coord);
    // Only the canonical stone terrain includes these mineral seams. Capability
    // registries without the complete authored mineral set retain their existing
    // homogeneous terrain behavior.
    let mineral_states = [
        crate::content::BLOCK_STATE_HEMATITE_ORE,
        crate::content::BLOCK_STATE_CASSITERITE_ORE,
        crate::content::BLOCK_STATE_SPHALERITE_ORE,
        crate::content::BLOCK_STATE_MAGNETITE_ORE,
        crate::content::BLOCK_STATE_GALENA_ORE,
        crate::content::BLOCK_STATE_PYRITE_ORE,
        crate::content::BLOCK_STATE_GRAPHITE_ORE,
        crate::content::BLOCK_STATE_QUARTZ_ORE,
    ];
    let mineral_enabled = profile.solid_state() == crate::content::BLOCK_STATE_STONE
        && mineral_states
            .iter()
            .all(|&ore| registry.get_state(ore).is_some());
    for local_z in 0..CHUNK_SIDE {
        for local_x in 0..CHUNK_SIDE {
            let x = min_x + local_x as i64;
            let z = min_z + local_z as i64;
            let surface = terrain_height(profile.seed(), x, z);
            for y in WORLD_MIN_Y..=surface {
                let position = VoxelCoord::new(x, y, z);
                if temple.is_some_and(|site| site.surface_seal(surface, position)) {
                    let seal = crate::content::BLOCK_STATE_TEMPLE_SEAL;
                    let seal_mass =
                        registry
                            .represented_mass_for_state(seal)
                            .unwrap_or_else(|| {
                                unreachable!("validated temple content supplies a seal")
                            });
                    chunk.set_block(position, seal, crate::core::quantity::Mass::ZERO, seal_mass);
                    continue;
                }
                if let Some(state) = temple.and_then(|site| site.generated_state(position)) {
                    if state != BlockStateId::AIR {
                        let relic_mass = registry
                            .represented_mass_for_state(state)
                            .unwrap_or_else(|| unreachable!("temple state is an authored block"));
                        chunk.set_block(
                            position,
                            state,
                            crate::core::quantity::Mass::ZERO,
                            relic_mass,
                        );
                    }
                    continue;
                }
                if mineral_enabled
                    && let Some(ore) =
                        seeded_mineral_state(profile.seed(), position, mineral_states)
                {
                    let ore = select_mineral_grade(registry, profile.seed(), position, ore);
                    let mass = registry
                        .represented_mass_for_state(ore)
                        .unwrap_or_else(|| unreachable!("all mineral states are registered"));
                    chunk.set_block(position, ore, crate::core::quantity::Mass::ZERO, mass);
                    continue;
                }
                chunk.set_block(
                    position,
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

/// 4 m mineral lenses at bounded deep elevations. This is seeded terrain
/// identity: no virtual deposits, no metal extracted from chemically bound ore,
/// and no repeated discovery rolls when the same chunk is reloaded.
pub(super) fn seeded_mineral_state(
    seed: WorldGenerationSeed,
    position: VoxelCoord,
    states: [BlockStateId; 8],
) -> Option<BlockStateId> {
    if !(-55..=-26).contains(&position.y()) {
        return None;
    }
    let hash = mix64(
        seed.value()
            ^ (position.x().div_euclid(4) as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
            ^ (position.y().div_euclid(4) as u64).wrapping_mul(0xD6E8_FEB8_6659_FD93)
            ^ (position.z().div_euclid(4) as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
            ^ 0x4D49_4E45_5241_4C53,
    );
    // 4 m lenses are sparse, and unlike a global mineral uniform roll,
    // their types reflect shallow vs deep host-rock position. Lattice hash
    // remains invariant under chunk and generation-order changes.
    hash.is_multiple_of(89).then(|| {
        let choice = ((hash >> 13) % 8) as usize;
        // Select strata using the lattice cell, not individual voxel elevation;
        // otherwise one 4 m lens crossing -40 splits into unrelated ores.
        let index = if position.y().div_euclid(4) >= -10 {
            // Upper deep rock: tin/zinc/lead/graphite/quartz plus iron oxide.
            [0, 1, 2, 4, 5, 6, 7, 7][choice]
        } else {
            // Lower deep rock: magnetite and sulfides dominate the rare lenses.
            [0, 0, 1, 2, 3, 3, 5, 5][choice]
        };
        states[index]
    })
}

fn select_mineral_grade(
    registry: &WorldRegistry,
    seed: WorldGenerationSeed,
    position: VoxelCoord,
    mineral: BlockStateId,
) -> BlockStateId {
    let graded = match mineral {
        crate::content::BLOCK_STATE_HEMATITE_ORE => {
            Some(crate::content::BLOCK_STATE_GRADED_HEMATITE_ORE)
        }
        crate::content::BLOCK_STATE_CASSITERITE_ORE => {
            Some(crate::content::BLOCK_STATE_GRADED_CASSITERITE_ORE)
        }
        crate::content::BLOCK_STATE_MAGNETITE_ORE => {
            Some(crate::content::BLOCK_STATE_GRADED_MAGNETITE_ORE)
        }
        crate::content::BLOCK_STATE_GALENA_ORE => {
            Some(crate::content::BLOCK_STATE_GRADED_GALENA_ORE)
        }
        _ => None,
    };
    graded
        .filter(|&state| {
            registry.get_state(state).is_some() && seeded_mineral_is_graded(seed, position)
        })
        .unwrap_or(mineral)
}

/// A second seeded lattice decision chooses rich vs gangue-bearing material in the same
/// four-metre body, preserving the original mineral species across all grade variants.
fn seeded_mineral_is_graded(seed: WorldGenerationSeed, position: VoxelCoord) -> bool {
    let hash = mix64(
        seed.value()
            ^ (position.x().div_euclid(4) as u64).wrapping_mul(0xAA9C_3E12_6B65_8D21)
            ^ (position.y().div_euclid(4) as u64).wrapping_mul(0xCA12_4D09_3578_F6EB)
            ^ (position.z().div_euclid(4) as u64).wrapping_mul(0xE9C3_56B1_420A_DD75)
            ^ 0x4752_4144_4544_4F52,
    );
    hash & 1 == 1
}

/// Newly generated solid cells must not conceal finite loose material that
/// was already assigned surface custody in an ungenerated column. Depleted
/// historical parcels have no physical residue to obstruct terrain admission.
fn first_surface_resource_collision(
    registry: &WorldRegistry,
    state: &AppState,
    chunk: &WorldChunk,
) -> Option<VoxelCoord> {
    state
        .surface()
        .available_resources_in_chunk(chunk.coord())
        .filter(|resource| {
            registry
                .block_for_state(chunk.block_at(resource.position()))
                .is_some_and(|block| block.collision() == super::BlockCollisionShape::FullCube)
        })
        // Report the lowest parcel ID when multiple resources conflict,
        // independently of spatial-index traversal order.
        .min_by_key(|resource| resource.id())
        .map(|resource| resource.position())
}

/// Finished stationary enclosure bodies are physical custody, unlike loose
/// unenclosed stockpiles. Only their actual located voxels obstruct a future
/// full-cube terrain block; no remote stockpile inventory is scanned.
fn first_stationary_enclosure_collision_in_generated_chunk(
    registry: &WorldRegistry,
    state: &AppState,
    chunk: &WorldChunk,
) -> Option<VoxelCoord> {
    state
        .logistics()
        .stockpiles_in_chunk(chunk.coord())
        .filter(|&(id, voxel)| {
            state
                .inventory()
                .get_stockpile(id)
                .is_some_and(|stockpile| stockpile.enclosure().is_some())
                && (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE).contains(&voxel.y())
                && registry
                    .block_for_state(chunk.block_at(voxel))
                    .is_some_and(|block| block.collision() == BlockCollisionShape::FullCube)
        })
        .min_by_key(|(id, _)| *id)
        .map(|(_, voxel)| voxel)
}

/// A built member has physical matter even when its narrow cross-section
/// does not fill the entire voxel. A newly generated full cube cannot
/// intersect any such portion. Limit the scan to the finite new chunk rather
/// than expanding potentially huge, user-authored member bounds.
fn first_structure_collision_in_generated_chunk(
    registry: &WorldRegistry,
    state: &AppState,
    chunk: &WorldChunk,
) -> Option<VoxelCoord> {
    let min_x = chunk.coord().world_min_x()?;
    let min_z = chunk.coord().world_min_z()?;
    let max_x = min_x + (CHUNK_SIDE - 1) as i64;
    let max_z = min_z + (CHUNK_SIDE - 1) as i64;
    state
        .structures()
        .elements_intersecting_horizontal_columns(min_x, max_x, min_z, max_z)
        .filter(|element| !element.embodied_material().is_empty())
        .find_map(|element| {
            first_solid_block_intersecting_bounds(registry, chunk, element.bounds())
        })
}

fn first_solid_block_intersecting_bounds(
    registry: &WorldRegistry,
    chunk: &WorldChunk,
    bounds: VoxelBounds,
) -> Option<VoxelCoord> {
    let min_x = chunk.coord().world_min_x()?;
    let min_z = chunk.coord().world_min_z()?;
    let last_x = min_x + (CHUNK_SIDE - 1) as i64;
    let last_z = min_z + (CHUNK_SIDE - 1) as i64;
    let start = bounds.min();
    let end = bounds.max_exclusive();
    let x0 = start.x().max(min_x);
    let x1 = (end.x() - 1).min(last_x);
    let y0 = start.y().max(WORLD_MIN_Y);
    let y1 = (end.y() - 1).min(WORLD_MAX_Y_EXCLUSIVE - 1);
    let z0 = start.z().max(min_z);
    let z1 = (end.z() - 1).min(last_z);
    if x0 > x1 || y0 > y1 || z0 > z1 {
        return None;
    }
    for z in z0..=z1 {
        for x in x0..=x1 {
            for y in y0..=y1 {
                let voxel = VoxelCoord::new(x, y, z);
                if registry
                    .block_for_state(chunk.block_at(voxel))
                    .is_some_and(|block| block.collision() == BlockCollisionShape::FullCube)
                {
                    return Some(voxel);
                }
            }
        }
    }
    None
}

/// Prospective materialization clearance. Inert plans may overlap stone;
/// embodying their material must first prove the entire reserved volume clear.
pub(crate) fn first_solid_block_in_structural_bounds(
    registries: &Registries,
    state: &AppState,
    bounds: VoxelBounds,
) -> Option<VoxelCoord> {
    state
        .world()
        .chunks()
        .find_map(|chunk| first_solid_block_intersecting_bounds(registries.world(), chunk, bounds))
}

/// Trusted-load counterpart of prospective terrain admission. Both paths
/// test the same physical bounds against exactly the existing block truth.
pub(crate) fn first_embodied_structure_collision_in_world(
    registries: &Registries,
    state: &AppState,
) -> Option<VoxelCoord> {
    state.world().chunks().find_map(|chunk| {
        first_structure_collision_in_generated_chunk(registries.world(), state, chunk)
    })
}

/// Trusted-load counterpart of prospective enclosure terrain admission.
/// Only generated block truth may bury a completed stationary enclosure.
pub(crate) fn first_buried_stationary_enclosure_in_world(
    registries: &Registries,
    state: &AppState,
) -> Option<VoxelCoord> {
    // This validation runs on every trusted load, including worlds with many
    // generated columns and no enclosures. Query each located enclosure against
    // the indexed block world once instead of scanning every generated column.
    // Preserve chunk-first, then stockpile-ID-first diagnostic ordering.
    state
        .logistics()
        .stockpile_locations()
        .filter(|&(stockpile, _)| {
            state
                .inventory()
                .get_stockpile(stockpile)
                .is_some_and(|record| record.enclosure().is_some())
        })
        .filter(|&(_, voxel)| {
            state
                .world()
                .block_at(voxel)
                .and_then(|block| registries.world().block_for_state(block))
                .is_some_and(|block| block.collision() == BlockCollisionShape::FullCube)
        })
        .min_by_key(|(stockpile, voxel)| (ChunkCoord::containing(*voxel), *stockpile))
        .map(|(_, voxel)| voxel)
}

fn temple_site_for_chunk(
    registry: &WorldRegistry,
    profile: WorldGenerationProfile,
    coord: ChunkCoord,
) -> Option<temple::TempleSite> {
    // Optional capability registries may omit this entire content set.
    // Never instantiate only half a temple and then unwrap missing relic mass.
    [
        crate::content::BLOCK_STATE_TEMPLE_ARCHIVE,
        crate::content::BLOCK_STATE_TEMPLE_DRIVE,
        crate::content::BLOCK_STATE_TEMPLE_SEAL,
        crate::content::BLOCK_STATE_TEMPLE_TIN_DATA_PLATE,
        crate::content::BLOCK_STATE_TEMPLE_IRON_BEARING_HOUSING,
        crate::content::BLOCK_STATE_TEMPLE_REFRACTORY_FIXTURE,
        crate::content::BLOCK_STATE_TEMPLE_ZINC_CORROSION_PANEL,
    ]
    .into_iter()
    .all(|state| registry.get_state(state).is_some())
    .then(|| temple::TempleSite::for_chunk(profile.seed(), coord))
    .flatten()
}

/// A not-yet-generated temple is already part of the deterministic world
/// identity. Check its finite resident footprint before publishing the chunk,
/// including when the prospective terrain cells themselves are air.
fn first_player_prospective_temple_collision(
    registries: &Registries,
    state: &AppState,
    profile: WorldGenerationProfile,
    coord: ChunkCoord,
) -> Option<VoxelCoord> {
    state.logistics().player()?;
    first_prospective_temple_overlap(registries.world(), profile, coord, |voxel| {
        crate::logistics::player_body_occupies_voxel(registries, state, voxel)
    })
}

fn first_infrastructure_prospective_temple_collision(
    registries: &Registries,
    state: &AppState,
    profile: WorldGenerationProfile,
    coord: ChunkCoord,
) -> Option<VoxelCoord> {
    first_prospective_temple_overlap(registries.world(), profile, coord, |voxel| {
        state.logistics().has_stationary_occupant_at(voxel)
            || state
                .structures()
                .elements_covering(voxel)
                .any(|element| !element.embodied_material().is_empty())
    })
}

/// Use one finite resident footprint for both player and durable-custody
/// admission. All 36 candidate bounding cells are checked without scanning
/// unrelated world history or constructing a temporary generated world.
fn first_prospective_temple_overlap(
    registry: &WorldRegistry,
    profile: WorldGenerationProfile,
    coord: ChunkCoord,
    mut overlaps: impl FnMut(VoxelCoord) -> bool,
) -> Option<VoxelCoord> {
    let site = temple_site_for_chunk(registry, profile, coord)?;
    let warden = site.warden();
    let archivist = site.archivist();
    for y in warden.y()..=warden.y() + 2 {
        for x in warden.x()..=warden.x() + 1 {
            for z in archivist.z() - 1..=warden.z() {
                let voxel = VoxelCoord::new(x, y, z);
                if crate::kobolds::initial_temple_caste_occupies_voxel(voxel, warden, archivist)
                    && overlaps(voxel)
                {
                    return Some(voxel);
                }
            }
        }
    }
    None
}

/// Resolves one site in an already-generated seeded world column. Absent
/// authored relic definitions never produce an ancient sanctuary.
fn temple_site_in_chunk(
    registries: &Registries,
    state: &AppState,
    coord: ChunkCoord,
) -> Option<temple::TempleSite> {
    let profile = state.world().generation_profile()?;
    // A seeded site is deterministic, but its heavy occupants do not exist
    // before the containing chunk is generated. Otherwise generating an
    // unrelated distant chunk can retroactively overlap an admitted player
    // at an unloaded sanctuary.
    state.world().get_chunk(coord)?;
    temple_site_for_chunk(registries.world(), profile, coord)
}

fn temple_site_at(
    registries: &Registries,
    state: &AppState,
    observer: VoxelCoord,
) -> Option<temple::TempleSite> {
    temple_site_in_chunk(registries, state, ChunkCoord::containing(observer))
}

/// Returns original temple resident identities, not their current mutable
/// positions. Kobold ownership resolves the warden's displacement; true
/// local sight prevents obstructed or distant actors from being revealed.
pub(crate) fn temple_residents_at(
    registries: &Registries,
    state: &AppState,
    observer: VoxelCoord,
) -> Option<[VoxelCoord; 2]> {
    let site = temple_site_at(registries, state, observer)?;
    Some([site.warden(), site.archivist()])
}

/// Resident candidates in the observer's column and its eight immediate neighbors.
/// A caste may be only a few cells away across a chunk boundary. Return only
/// already-generated sanctuaries; callers still enforce their own physical
/// observation radius or occluded sight before any behavior or presentation.
/// No world-wide temple index or distant generation is consulted.
pub(crate) fn nearby_temple_residents<'a>(
    registries: &'a Registries,
    state: &'a AppState,
    observer: VoxelCoord,
) -> impl Iterator<Item = [VoxelCoord; 2]> + 'a {
    let origin = ChunkCoord::containing(observer);
    (-1_i64..=1).flat_map(move |dx| {
        (-1_i64..=1).filter_map(move |dz| {
            let chunk = ChunkCoord::new(origin.x().checked_add(dx)?, origin.z().checked_add(dz)?);
            temple_site_in_chunk(registries, state, chunk)
                .map(|site| [site.warden(), site.archivist()])
        })
    })
}

/// Three physically adjacent cells, entirely inside a generated ancient hall.
/// The caller still checks collision and line of sight before moving a resident.
pub(crate) fn temple_warden_route_at(
    registries: &Registries,
    state: &AppState,
    position: VoxelCoord,
) -> Option<[VoxelCoord; 3]> {
    temple_site_at(registries, state, position).map(temple::TempleSite::warden_route)
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
