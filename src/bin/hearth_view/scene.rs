//! World creation through production generation, then parallel renderer-neutral meshing.

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use deep_hearth::content::BLOCK_STATE_STONE;
use deep_hearth::core::time::{Season, SimulationTick};
use deep_hearth::meshing::{SectionMesh, SectionMesher};
use deep_hearth::shader::BakedShaderSet;
use deep_hearth::spatial::VoxelCoord;
use deep_hearth::texture::BakedTextureArray;
use deep_hearth::world::{
    ChunkCoord, WorldGenerationSeed, select_natural_world_spawn, validate_generate_natural_chunk,
};
use deep_hearth::{AppState, Registries, build_registries};

use crate::args::Options;

const SECTION_SIDE: i64 = 16;
/// Standing eye height above the spawn cell floor, in blocks.
const EYE_HEIGHT: f32 = 1.62;

/// One drawable mesh with an origin-relative bounding box for frustum culling.
pub struct Placed {
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub mesh: SectionMesh,
}

pub struct Scene {
    pub registries: Registries,
    pub state: AppState,
    pub textures: BakedTextureArray,
    pub shaders: BakedShaderSet,
    /// Integer render origin; every mesh position is relative to this cell.
    pub origin: VoxelCoord,
    pub chunks: Vec<ChunkCoord>,
    pub terrain: Vec<Placed>,
    pub trees: Vec<Placed>,
    pub tick: SimulationTick,
}

fn mix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

impl Scene {
    pub fn generate(options: &Options) -> Result<Self, String> {
        let started = Instant::now();
        let registries = build_registries();
        let seed = WorldGenerationSeed::new(options.seed);
        let (mut state, spawn) = find_spawn(&registries, seed, options.seed)?;
        let center = ChunkCoord::containing(spawn);
        let mut chunks = Vec::new();
        for dx in -options.radius..=options.radius {
            for dz in -options.radius..=options.radius {
                let chunk = ChunkCoord::new(center.x() + dx, center.z() + dz);
                generate_chunk(&registries, &mut state, seed, chunk)?;
                chunks.push(chunk);
            }
        }
        eprintln!(
            "generated {} chunks around {spawn:?} in {:.2?}",
            chunks.len(),
            started.elapsed()
        );
        let textures = registries.textures().bake_texture_array();
        let shaders = registries.shaders().bake_shader_set();
        let tick = display_tick(&registries, options.season, options.hour);
        let mut scene = Self {
            registries,
            state,
            textures,
            shaders,
            origin: spawn,
            chunks,
            terrain: Vec::new(),
            trees: Vec::new(),
            tick,
        };
        scene.remesh_terrain();
        scene.remesh_trees();
        Ok(scene)
    }

    /// Origin-relative eye position of a standing player at the spawn cell.
    pub fn spawn_eye(&self) -> [f32; 3] {
        [0.5, EYE_HEIGHT, 0.5]
    }

    pub fn ticks_per_day(&self) -> u64 {
        self.registries.core().calendar().ticks_per_day()
    }

    /// Fraction of the simulation day elapsed since dawn.
    pub fn day_fraction(&self) -> f32 {
        let day = self.ticks_per_day();
        (self.tick.value() % day) as f32 / day as f32
    }

    fn remesh_terrain(&mut self) {
        let started = Instant::now();
        let jobs: Vec<(ChunkCoord, i64)> = self
            .chunks
            .iter()
            .filter_map(|&chunk| self.state.world().get_chunk(chunk))
            .flat_map(|column| {
                let chunk = column.coord();
                column.sections().map(move |section| (chunk, section.y()))
            })
            .collect();
        let mesher = SectionMesher::new(&self.registries, &self.textures);
        let world = self.state.world();
        let origin = self.origin;
        self.terrain = parallel_map(&jobs, |&(chunk, y)| {
            let mesh = mesher.mesh_section(world, chunk, y, origin);
            let min = relative(
                origin,
                [
                    chunk.x() * SECTION_SIDE,
                    y * SECTION_SIDE,
                    chunk.z() * SECTION_SIDE,
                ],
            );
            let side = SECTION_SIDE as f32;
            (!mesh.is_empty()).then(|| Placed {
                min,
                max: [min[0] + side, min[1] + side, min[2] + side],
                mesh,
            })
        });
        let quads: usize = self
            .terrain
            .iter()
            .map(|placed| {
                (placed.mesh.opaque().vertices().len() + placed.mesh.cutout().vertices().len()) / 4
            })
            .sum();
        eprintln!(
            "meshed {} sections into {} visible ({quads} quads) in {:.2?}",
            jobs.len(),
            self.terrain.len(),
            started.elapsed()
        );
    }

    fn remesh_trees(&mut self) {
        let mesher = SectionMesher::new(&self.registries, &self.textures);
        let world = self.state.world();
        let (origin, tick) = (self.origin, self.tick);
        self.trees = parallel_map(&self.chunks, |&chunk| {
            let mesh = mesher.mesh_chunk_trees(world, chunk, tick, origin);
            let min = relative(
                origin,
                [chunk.x() * SECTION_SIDE, -64, chunk.z() * SECTION_SIDE],
            );
            let side = SECTION_SIDE as f32;
            // Crowns may overhang the root chunk by their radius.
            (!mesh.is_empty()).then(|| Placed {
                min: [min[0] - 4.0, min[1], min[2] - 4.0],
                max: [min[0] + side + 4.0, min[1] + 384.0, min[2] + side + 4.0],
                mesh,
            })
        });
    }
}

fn relative(origin: VoxelCoord, cell: [i64; 3]) -> [f32; 3] {
    [
        (cell[0] - origin.x()) as f32,
        (cell[1] - origin.y()) as f32,
        (cell[2] - origin.z()) as f32,
    ]
}

/// Searches seeded districts for a safely occupiable natural standing cell, as world creation does.
fn find_spawn(
    registries: &Registries,
    seed: WorldGenerationSeed,
    raw_seed: u64,
) -> Result<(AppState, VoxelCoord), String> {
    for attempt in 0..64_u64 {
        let salt = mix64(raw_seed ^ attempt.wrapping_mul(0xD1B5_4A32_D192_ED03));
        let chunk = ChunkCoord::new((salt % 49) as i64 - 24, ((salt >> 20) % 49) as i64 - 24);
        let mut state = AppState::new();
        generate_chunk(registries, &mut state, seed, chunk)?;
        if let Some(spawn) =
            select_natural_world_spawn(registries, &state, chunk, mix64(salt), None)
        {
            return Ok((state, spawn));
        }
    }
    Err("no occupiable natural terrain in 64 seeded districts".to_owned())
}

fn generate_chunk(
    registries: &Registries,
    state: &mut AppState,
    seed: WorldGenerationSeed,
    chunk: ChunkCoord,
) -> Result<(), String> {
    if state.world().get_chunk(chunk).is_some() {
        return Ok(());
    }
    validate_generate_natural_chunk(registries, state, seed, chunk, BLOCK_STATE_STONE)
        .map_err(|error| format!("terrain generation failed at {chunk:?}: {error}"))?
        .commit(state)
        .map(|_| ())
        .map_err(|error| format!("terrain commit failed at {chunk:?}: {error}"))
}

/// First tick of the requested season's middle day, advanced to the requested hour after dawn.
fn display_tick(registries: &Registries, season: Season, hour: f32) -> SimulationTick {
    let calendar = registries.core().calendar();
    let day = calendar.ticks_per_day();
    let days: Vec<u64> = (0..2_000_u64)
        .filter(|&index| calendar.season_at(SimulationTick::new(index * day)) == season)
        .take_while({
            let mut previous = None;
            move |&index| {
                let contiguous = previous.is_none_or(|last| index == last + 1);
                previous = Some(index);
                contiguous
            }
        })
        .collect();
    let middle = days.get(days.len() / 2).copied().unwrap_or(0);
    let offset = ((hour / 24.0).clamp(0.0, 0.9999) * day as f32) as u64;
    SimulationTick::new(middle * day + offset)
}

/// Deterministic-order parallel map; empty results are dropped.
fn parallel_map<T: Sync, R: Send>(items: &[T], work: impl Fn(&T) -> Option<R> + Sync) -> Vec<R> {
    let next = AtomicUsize::new(0);
    let results: Mutex<Vec<(usize, R)>> = Mutex::new(Vec::new());
    let workers = std::thread::available_parallelism().map_or(4, usize::from);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                let mut local = Vec::new();
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(item) = items.get(index) else { break };
                    if let Some(result) = work(item) {
                        local.push((index, result));
                    }
                }
                results
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .extend(local);
            });
        }
    });
    let mut results = results
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    results.sort_unstable_by_key(|(index, _)| *index);
    results.into_iter().map(|(_, result)| result).collect()
}
