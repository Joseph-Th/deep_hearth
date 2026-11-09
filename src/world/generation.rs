//! Deterministic terrain identity, explicit chunk generation, and player-neighborhood demand.

mod errors;
pub use errors::{ChunkGenerationCommitError, ChunkGenerationError};
mod forage;
pub(crate) use forage::seeded_perennial_matches;
mod habitat;
mod minerals;
mod occupancy;
mod springs;
pub(super) mod temple;
mod temple_presence;
pub(in crate::world) mod terrain;

pub(super) use habitat::habitat_at;
pub use habitat::{HabitatKind, assess_player_habitat};
pub(super) use minerals::seeded_mineral_state;
use minerals::{seeded_surface_lode_state, select_mineral_grade};
use occupancy::validate_generated_chunk_occupancy;
pub(crate) use occupancy::{
    first_buried_stationary_enclosure_in_world, first_embodied_structure_collision_in_world,
    first_solid_block_in_structural_bounds,
};
use temple_presence::temple_site_for_chunk;
pub(crate) use temple_presence::{
    nearby_temple_residents, temple_residents_at, temple_warden_route_at,
};
use terrain::mix64;
pub(super) use terrain::terrain_height;

use serde::{Deserialize, Serialize};

use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::fluid::{
    FluidGenerationCommitError, FluidGenerationError, ValidatedFluidGenerationBatch,
    validate_generate_fluid_stores,
};
use crate::player::{PlayerAdmissionState, player_admission_state};
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::surface::{
    SurfaceResourceCreationError, SurfaceResourceSpec, ValidatedSurfaceResourceCreation,
};

use super::{BlockStateId, CHUNK_SIDE, ChunkCoord, WORLD_MIN_Y, WorldChunk, WorldRegistry};

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

/// Common optimistic transaction guard for single-chunk and neighborhood
/// generation. Both admissions inspect the same owners, so neither route may
/// commit terrain against stale physical occupancy or reserved custody.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ChunkGenerationCommitGuard {
    world_revision: u64,
    surface_revision: u64,
    logistics_revision: u64,
    inventory_revision: u64,
    structure_revision: u64,
    player_work_revision: u64,
    opening_residents: Option<[VoxelCoord; 2]>,
}

impl ChunkGenerationCommitGuard {
    fn capture(state: &AppState) -> Self {
        Self {
            world_revision: state.world().revision(),
            surface_revision: state.surface().revision(),
            logistics_revision: state.logistics().revision(),
            inventory_revision: state.inventory().revision(),
            structure_revision: state.structures().revision(),
            player_work_revision: state.player_work().revision(),
            opening_residents: crate::kobolds::opening_kobold_positions(state),
        }
    }

    fn precheck(
        &self,
        state: &AppState,
        chunks: &[WorldChunk],
    ) -> Result<(), ChunkGenerationCommitError> {
        let actual = state.world().revision();
        if actual != self.world_revision {
            return Err(ChunkGenerationCommitError::StaleWorldRevision {
                expected: self.world_revision,
                actual,
            });
        }
        let actual = state.surface().revision();
        if actual != self.surface_revision {
            return Err(ChunkGenerationCommitError::StaleSurfaceRevision {
                expected: self.surface_revision,
                actual,
            });
        }
        let actual = state.logistics().revision();
        if actual != self.logistics_revision {
            return Err(ChunkGenerationCommitError::StaleLogisticsRevision {
                expected: self.logistics_revision,
                actual,
            });
        }
        let actual = state.structures().revision();
        if actual != self.structure_revision {
            return Err(ChunkGenerationCommitError::StaleStructureRevision {
                expected: self.structure_revision,
                actual,
            });
        }
        let actual = state.player_work().revision();
        if actual != self.player_work_revision {
            return Err(ChunkGenerationCommitError::StalePlayerWorkRevision {
                expected: self.player_work_revision,
                actual,
            });
        }
        let actual = state.inventory().revision();
        if actual != self.inventory_revision {
            return Err(ChunkGenerationCommitError::StaleInventoryRevision {
                expected: self.inventory_revision,
                actual,
            });
        }
        validate_unchanged_opening_residents(state, self.opening_residents, chunks)
    }
}

#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedChunkGeneration {
    guard: ChunkGenerationCommitGuard,
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
        let coord = self.chunk.coord();
        self.guard
            .precheck(state, std::slice::from_ref(&self.chunk))?;
        if state.world().get_chunk(coord).is_some() {
            return Err(ChunkGenerationCommitError::ChunkAppeared { chunk: coord });
        }
        state.world_state_mut().insert_generated_chunks(
            self.guard.world_revision,
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
    guard: ChunkGenerationCommitGuard,
    next_revision: u64,
    profile: WorldGenerationProfile,
    chunks: Vec<WorldChunk>,
    forage: Option<ValidatedSurfaceResourceCreation>,
    springs: Option<(ValidatedFluidGenerationBatch, Vec<VoxelCoord>, u64)>,
}

impl ValidatedPlayerChunkDemand {
    pub fn chunks(&self) -> impl ExactSizeIterator<Item = &WorldChunk> {
        self.chunks.iter()
    }

    pub fn commit(
        self,
        state: &mut AppState,
    ) -> Result<Vec<ChunkCoord>, ChunkGenerationCommitError> {
        self.guard.precheck(state, &self.chunks)?;
        if let Some((generation, _, _)) = self.springs.as_ref() {
            generation.precheck(state).map_err(|error| match error {
                FluidGenerationCommitError::Tick { expected, actual } => {
                    ChunkGenerationCommitError::SpringClockChanged { expected, actual }
                }
                FluidGenerationCommitError::Revision { expected, actual } => {
                    ChunkGenerationCommitError::StaleFluidRevision { expected, actual }
                }
                FluidGenerationCommitError::NextStoreId { expected, actual } => {
                    ChunkGenerationCommitError::StaleFluidIdentity { expected, actual }
                }
            })?;
        }
        for chunk in &self.chunks {
            if state.world().get_chunk(chunk.coord()).is_some() {
                return Err(ChunkGenerationCommitError::ChunkAppeared {
                    chunk: chunk.coord(),
                });
            }
        }
        // The common terrain guard already binds surface, world and structure
        // revisions. These source parcels are dated to tick zero, so a distant
        // clock advance cannot stale their physical generation identity.
        if let Some(forage) = self.forage {
            forage.apply(state);
        }
        if let Some((generation, positions, next_logistics_revision)) = self.springs {
            let locations = generation.store_ids().zip(positions).collect::<Vec<_>>();
            generation.apply(state, &locations);
            state
                .logistics_state_mut()
                .apply_generated_spring_locations(
                    self.guard.logistics_revision,
                    next_logistics_revision,
                    locations,
                );
        }
        let coords = self
            .chunks
            .iter()
            .map(WorldChunk::coord)
            .collect::<Vec<_>>();
        state.world_state_mut().insert_generated_chunks(
            self.guard.world_revision,
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
        guard: ChunkGenerationCommitGuard::capture(state),
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
    // Plan each seeded forage site once. Springs must use exactly the sites
    // authorized for this expansion, not a separate regeneration of the same
    // candidate choices with potentially diverging occupancy assumptions.
    let (forage, forage_sites) = validate_forage_for_chunks(registries, state, profile, &chunks)?;
    let springs = validate_springs_for_chunks(registries, state, profile, &chunks, &forage_sites)?;
    if !state.can_spend_world_revisions(1) {
        return Err(ChunkGenerationError::WorldRevisionExhausted);
    }
    let expected_revision = state.world().revision();
    let next_revision = expected_revision
        .checked_add(1)
        .unwrap_or_else(|| unreachable!("world headroom check includes player chunk demand"));
    Ok(Some(ValidatedPlayerChunkDemand {
        guard: ChunkGenerationCommitGuard::capture(state),
        next_revision,
        profile,
        chunks,
        forage,
        springs,
    }))
}

fn validate_springs_for_chunks(
    registries: &Registries,
    state: &AppState,
    profile: WorldGenerationProfile,
    chunks: &[WorldChunk],
    forage_sites: &[Option<VoxelCoord>],
) -> Result<Option<(ValidatedFluidGenerationBatch, Vec<VoxelCoord>, u64)>, ChunkGenerationError> {
    let sources = chunks
        .iter()
        .zip(forage_sites)
        .filter_map(|(chunk, forage_site)| {
            let blocked = forage_site.as_ref().map_or(&[][..], std::slice::from_ref);
            springs::seeded_spring_spec(state, profile, chunk, blocked)
        })
        .collect::<Vec<_>>();
    if sources.is_empty() {
        return Ok(None);
    }
    if !state.can_spend_logistics_revisions(1) {
        return Err(ChunkGenerationError::LogisticsRevisionExhausted);
    }
    let next_logistics_revision = state
        .logistics()
        .revision()
        .checked_add(1)
        .ok_or(ChunkGenerationError::LogisticsRevisionExhausted)?;
    let (positions, specs): (Vec<_>, Vec<_>) = sources.into_iter().unzip();
    let batch =
        validate_generate_fluid_stores(registries, state, specs).map_err(|error| match error {
            FluidGenerationError::IdExhausted => ChunkGenerationError::SpringIdExhausted,
            FluidGenerationError::RevisionExhausted => {
                ChunkGenerationError::SpringRevisionExhausted
            }
            FluidGenerationError::ZeroCapacity
            | FluidGenerationError::UnknownDefinition { .. }
            | FluidGenerationError::ZeroVolume
            | FluidGenerationError::ZeroTemperature
            | FluidGenerationError::VolumeExceedsCapacity { .. }
            | FluidGenerationError::BelowMeltingPoint { .. } => {
                ChunkGenerationError::InvalidSpringWater
            }
        })?;
    Ok(Some((batch, positions, next_logistics_revision)))
}

/// One surface-owner admission for all newly generated natural food patches.
/// The biological source existed at world creation, even if a distant column
/// only becomes persistent later; exploration does not refresh its shelf age.
fn validate_forage_for_chunks(
    registries: &Registries,
    state: &AppState,
    profile: WorldGenerationProfile,
    chunks: &[WorldChunk],
) -> Result<
    (
        Option<ValidatedSurfaceResourceCreation>,
        Vec<Option<VoxelCoord>>,
    ),
    ChunkGenerationError,
> {
    let specs = chunks
        .iter()
        .map(|chunk| forage::seeded_forage_spec(registries, state, profile, chunk))
        .collect::<Vec<_>>();
    let sites = specs
        .iter()
        .map(|spec| spec.as_ref().map(SurfaceResourceSpec::position))
        .collect::<Vec<_>>();
    if specs.iter().all(Option::is_none) {
        return Ok((None, sites));
    }
    crate::surface::validate_create_surface_resources_at(
        registries,
        state,
        specs.into_iter().flatten(),
        SimulationTick::ZERO,
    )
    .map(|forage| (Some(forage), sites))
    .map_err(|error| match error {
        SurfaceResourceCreationError::Empty => unreachable!("forage generation has nonempty specs"),
        SurfaceResourceCreationError::IdExhausted => ChunkGenerationError::ForageIdExhausted,
        SurfaceResourceCreationError::RevisionExhausted => {
            ChunkGenerationError::ForageRevisionExhausted
        }
        SurfaceResourceCreationError::InvalidResource(error) => {
            ChunkGenerationError::InvalidForageResource(error)
        }
    })
}

fn validate_generation_profile(
    registry: &WorldRegistry,
    state: &AppState,
    requested: WorldGenerationProfile,
) -> Result<(), ChunkGenerationError> {
    if requested.solid_state() == BlockStateId::AIR {
        return Err(ChunkGenerationError::AirGenerationSolidState);
    }
    if registry.get_state(requested.solid_state()).is_none() {
        return Err(ChunkGenerationError::UnknownSolidState {
            state: requested.solid_state(),
        });
    }
    // A registered non-air state is not necessarily terrain. An empty collision
    // shape or massless full-cube fixture would establish absent ground or an
    // impassable world without physical matter to excavate or account for.
    if !registry.is_material_backed_full_cube_state(requested.solid_state()) {
        return Err(ChunkGenerationError::InvalidTerrainSubstrate {
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
    let (min_x, min_z) = coord
        .bounded_world_origin()
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
            let surface_lode = mineral_enabled
                .then(|| seeded_surface_lode_state(profile.seed(), x, z, mineral_states))
                .flatten();
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
                // A visible outcrop connects exploratory observations to a real
                // finite mineral body. Ordinary mining and excavation remove its
                // physical blocks; later visits never regenerate those cells.
                let shallow_ore = (y >= surface - 3).then_some(surface_lode).flatten();
                if let Some(ore) = shallow_ore.or_else(|| {
                    mineral_enabled
                        .then(|| seeded_mineral_state(profile.seed(), position, mineral_states))
                        .flatten()
                }) {
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

#[cfg(test)]
#[path = "generation/equipment_occupancy_tests.rs"]
mod equipment_occupancy_tests;

#[cfg(test)]
#[path = "generation/profile_tests.rs"]
mod profile_tests;
