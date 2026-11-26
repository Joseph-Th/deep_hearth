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
    CHUNK_SIDE, ChunkCoord, SECTION_SIDE, WorldGenerationSeed, generate_natural_world_start,
    validate_generate_natural_chunk,
};
use deep_hearth::{AppState, Registries, build_registries};

use crate::args::Options;

#[cfg(test)]
#[path = "scene_tests.rs"]
mod tests;

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

impl Scene {
    pub fn generate(options: &Options) -> Result<Self, String> {
        let started = Instant::now();
        let registries = build_registries();
        let seed = WorldGenerationSeed::new(options.seed);
        let (mut state, spawn) = generate_natural_world_start(&registries, seed, None)
            .map_err(|error| error.to_string())?;
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
        let height = self
            .registries
            .survival()
            .physiology()
            .locomotion()
            .standing_eye_height();
        [0.5, height.micrometers() as f32 / 1_000_000.0, 0.5]
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
                    chunk.x() * CHUNK_SIDE as i64,
                    y * SECTION_SIDE as i64,
                    chunk.z() * CHUNK_SIDE as i64,
                ],
            );
            (!mesh.is_empty()).then(|| Placed {
                min,
                max: [
                    min[0] + CHUNK_SIDE as f32,
                    min[1] + SECTION_SIDE as f32,
                    min[2] + CHUNK_SIDE as f32,
                ],
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
            // Use the actual rendered extent, including overhanging crowns.
            let (min, max) = mesh_bounds(&mesh)?;
            Some(Placed { min, max, mesh })
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

fn mesh_bounds(mesh: &SectionMesh) -> Option<([f32; 3], [f32; 3])> {
    let mut positions = mesh
        .opaque()
        .vertices()
        .iter()
        .chain(mesh.cutout().vertices())
        .map(|vertex| vertex.position());
    let first = positions.next()?;
    let mut min = first;
    let mut max = first;
    for position in positions {
        for axis in 0..3 {
            min[axis] = min[axis].min(position[axis]);
            max[axis] = max[axis].max(position[axis]);
        }
    }
    Some((min, max))
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
