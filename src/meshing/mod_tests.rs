//! Owner tests for renderer-neutral section and tree meshing.

use crate::content::{BLOCK_STATE_STONE, build_registries};
use crate::core::state::AppState;
use crate::core::time::{Season, SimulationTick};
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::texture::BakedTextureArray;
use crate::world::{
    BlockStateId, ChunkCoord, SECTION_SIDE, WorldGenerationSeed, set_generated_block_for_test,
    validate_generate_natural_chunk,
};

use super::*;

const SEED: WorldGenerationSeed = WorldGenerationSeed::new(0x4D45_5348_4552);

fn generated(chunks: &[ChunkCoord]) -> (Registries, AppState) {
    let registries = build_registries();
    let mut state = AppState::new();
    for &chunk in chunks {
        validate_generate_natural_chunk(&registries, &state, SEED, chunk, BLOCK_STATE_STONE)
            .unwrap_or_else(|error| panic!("generation failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("generation commit failed: {error}"));
    }
    (registries, state)
}

fn neighborhood() -> Vec<ChunkCoord> {
    (-1..=1)
        .flat_map(|x| (-1..=1).map(move |z| ChunkCoord::new(x, z)))
        .collect()
}

fn section_ys(state: &AppState, chunk: ChunkCoord) -> Vec<i64> {
    state
        .world()
        .get_chunk(chunk)
        .map(|column| column.sections().map(|section| section.y()).collect())
        .unwrap_or_default()
}

fn opaque(registries: &Registries, state: &AppState, cell: VoxelCoord) -> bool {
    if cell.y() < crate::world::WORLD_MIN_Y {
        return true;
    }
    state
        .world()
        .block_at(cell)
        .and_then(|block| registries.world().block_for_state(block))
        .is_some_and(|block| block.is_opaque())
}

/// Brute-force count of opaque-cell faces that touch a non-opaque neighbor.
fn exposed_faces(registries: &Registries, state: &AppState, chunk: ChunkCoord, y: i64) -> usize {
    let side = SECTION_SIDE as i64;
    let (min_x, min_z, min_y) = (chunk.x() * side, chunk.z() * side, y * side);
    let mut faces = 0;
    for x in min_x..min_x + side {
        for y in min_y..min_y + side {
            for z in min_z..min_z + side {
                if !opaque(registries, state, VoxelCoord::new(x, y, z)) {
                    continue;
                }
                faces += [
                    (1, 0, 0),
                    (-1, 0, 0),
                    (0, 1, 0),
                    (0, -1, 0),
                    (0, 0, 1),
                    (0, 0, -1),
                ]
                .into_iter()
                .filter(|(dx, dy, dz)| {
                    !opaque(registries, state, VoxelCoord::new(x + dx, y + dy, z + dz))
                })
                .count();
            }
        }
    }
    faces
}

/// Total unit-face area covered by a mesh's quads (each quad is four consecutive vertices).
fn covered_area(mesh: &SurfaceMesh) -> f32 {
    mesh.vertices()
        .as_chunks::<4>()
        .0
        .iter()
        .map(|quad| {
            let [a, b, _, d] = quad.map(|v| v.position());
            let edge = |p: [f32; 3], q: [f32; 3]| {
                ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt()
            };
            edge(a, b) * edge(a, d)
        })
        .sum()
}

fn bake(registries: &Registries) -> BakedTextureArray {
    registries.textures().bake_texture_array()
}

#[test]
fn boundary_chunk_mesh_does_not_wrap_or_overflow_lighting_margin() {
    let side = SECTION_SIDE as i64;
    let chunk = ChunkCoord::new(i64::MIN.div_euclid(side), i64::MAX.div_euclid(side));
    let registries = build_registries();
    let mut state = AppState::new();
    crate::world::validate_generate_chunk(&registries, &state, SEED, chunk, BLOCK_STATE_STONE)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let textures = bake(&registries);
    let mesher = SectionMesher::new(&registries, &textures);
    let origin = VoxelCoord::new(
        chunk.world_min_x().unwrap(),
        0,
        chunk.world_min_z().unwrap(),
    );
    let mut vertices = 0;
    for y in section_ys(&state, chunk) {
        let mesh = mesher.mesh_section(state.world(), chunk, y, origin);
        for vertex in mesh.opaque().vertices() {
            let [x, _, z] = vertex.position();
            assert!((0.0..=side as f32).contains(&x));
            assert!((0.0..=side as f32).contains(&z));
            vertices += 1;
        }
    }
    assert!(vertices > 0);
    let _ = mesher.mesh_chunk_trees(state.world(), chunk, SimulationTick::ZERO, origin);
}

#[test]
fn greedy_section_mesh_covers_exactly_the_exposed_terrain_faces() {
    let (registries, state) = generated(&neighborhood());
    let textures = bake(&registries);
    let mesher = SectionMesher::new(&registries, &textures);
    let center = ChunkCoord::new(0, 0);
    let mut total_faces = 0;
    let mut total_quads = 0;
    for y in section_ys(&state, center) {
        let mesh = mesher.mesh_section(state.world(), center, y, VoxelCoord::new(0, 0, 0));
        let faces = exposed_faces(&registries, &state, center, y);
        let area = covered_area(mesh.opaque()) + covered_area(mesh.cutout());
        assert!(
            (area - faces as f32).abs() < 0.01,
            "section {y}: mesh covers {area} faces but {faces} are exposed"
        );
        total_faces += faces;
        total_quads += mesh.opaque().vertices().len() / 4;
    }
    assert!(total_faces > 0, "generated terrain must expose a surface");
    assert!(
        total_quads < total_faces,
        "greedy merging must emit fewer quads ({total_quads}) than unit faces ({total_faces})"
    );
}

#[test]
fn meshing_is_deterministic_and_encodes_the_surface_vertex_contract() {
    let (registries, state) = generated(&neighborhood());
    let textures = bake(&registries);
    let mesher = SectionMesher::new(&registries, &textures);
    let center = ChunkCoord::new(0, 0);
    let origin = VoxelCoord::new(3, 70, -5);
    for y in section_ys(&state, center) {
        let first = mesher.mesh_section(state.world(), center, y, origin);
        let second = mesher.mesh_section(state.world(), center, y, origin);
        assert_eq!(first, second);
        let opaque = first.opaque();
        assert_eq!(
            opaque.vertex_bytes().len(),
            opaque.vertices().len() * SURFACE_VERTEX_STRIDE
        );
        assert_eq!(opaque.index_bytes().len(), opaque.indices().len() * 4);
        assert_eq!(opaque.indices().len() * 4, opaque.vertices().len() * 6);
        if let Some(vertex) = opaque.vertices().first() {
            let bytes = opaque.vertex_bytes();
            let key_offset = SURFACE_VERTEX_OFFSETS[3];
            assert_eq!(
                u32::from_le_bytes([
                    bytes[key_offset],
                    bytes[key_offset + 1],
                    bytes[key_offset + 2],
                    bytes[key_offset + 3],
                ]),
                vertex.texture_key()
            );
        }
    }
}

#[test]
fn a_roof_shades_the_ground_beneath_it_while_open_ground_stays_sky_lit() {
    let (registries, mut state) = generated(&neighborhood());
    let surface = |state: &AppState, x: i64, z: i64| {
        (crate::world::WORLD_MIN_Y..crate::world::WORLD_MAX_Y_EXCLUSIVE)
            .rev()
            .find(|&y| opaque(&registries, state, VoxelCoord::new(x, y, z)))
            .unwrap_or_else(|| panic!("generated column has ground"))
    };
    let ground = surface(&state, 8, 8);
    let open_ground = surface(&state, 0, 0);
    // A 9x9 stone slab four cells above the ground shades the column at its center.
    for x in 4..=12 {
        for z in 4..=12 {
            let roof = VoxelCoord::new(x, ground.max(surface(&state, x, z)) + 4, z);
            set_generated_block_for_test(registries.world(), &mut state, roof, BLOCK_STATE_STONE);
        }
    }
    let textures = bake(&registries);
    let mesher = SectionMesher::new(&registries, &textures);
    let top_light_at = |state: &AppState, x: i64, y: i64, z: i64| {
        let section = y.div_euclid(SECTION_SIDE as i64);
        let mesh = mesher.mesh_section(
            state.world(),
            ChunkCoord::new(0, 0),
            section,
            VoxelCoord::new(0, 0, 0),
        );
        mesh.opaque()
            .vertices()
            .as_chunks::<4>()
            .0
            .iter()
            .find(|quad| {
                let covers = |axis: usize, value: i64| {
                    let low = quad
                        .iter()
                        .map(|v| v.position()[axis])
                        .fold(f32::MAX, f32::min);
                    let high = quad
                        .iter()
                        .map(|v| v.position()[axis])
                        .fold(f32::MIN, f32::max);
                    low <= value as f32 && (value + 1) as f32 <= high
                };
                quad[0].normal() == [0.0, 1.0, 0.0]
                    && quad[0].position()[1] == (y + 1) as f32
                    && covers(0, x)
                    && covers(2, z)
            })
            .map(|quad| quad.iter().map(|v| v.light()[0]).fold(0.0, f32::max))
            .unwrap_or_else(|| panic!("ground top face at {x},{z} is meshed"))
    };
    let shaded = top_light_at(&state, 8, ground, 8);
    let open = top_light_at(&state, 0, open_ground, 0);
    assert_eq!(open, 1.0, "open ground receives full skylight");
    assert!(shaded < 0.5, "roofed ground must be darker, got {shaded}");
    assert!(shaded > 0.0, "skylight spreads under a low open-sided roof");
}

#[test]
fn unexposed_and_ungenerated_sections_emit_nothing() {
    let (registries, state) = generated(&neighborhood());
    let textures = bake(&registries);
    let mesher = SectionMesher::new(&registries, &textures);
    let origin = VoxelCoord::new(0, 0, 0);
    let deepest = section_ys(&state, ChunkCoord::new(0, 0))
        .into_iter()
        .min()
        .unwrap_or_else(|| panic!("generated chunk has sections"));
    let floor = mesher.mesh_section(state.world(), ChunkCoord::new(0, 0), deepest, origin);
    assert_eq!(
        exposed_faces(&registries, &state, ChunkCoord::new(0, 0), deepest),
        (covered_area(floor.opaque()) + 0.5) as usize
    );
    assert!(
        mesher
            .mesh_section(state.world(), ChunkCoord::new(9, 9), 4, origin)
            .is_empty()
    );
    assert!(
        mesher
            .mesh_section(state.world(), ChunkCoord::new(0, 0), 19, origin)
            .is_empty()
    );
}

fn first_tree_chunk(state: &AppState) -> ChunkCoord {
    state
        .world()
        .trees()
        .map(|tree| ChunkCoord::containing(tree.root()))
        .min()
        .unwrap_or_else(|| panic!("natural neighborhood contains a living tree"))
}

fn tick_in(registries: &Registries, season: Season) -> SimulationTick {
    let calendar = registries.core().calendar();
    (0..)
        .map(|day| SimulationTick::new(day * calendar.ticks_per_day()))
        .find(|&tick| calendar.season_at(tick) == season)
        .unwrap_or_else(|| panic!("calendar reaches every season"))
}

#[test]
fn trees_show_solid_trunks_and_seasonal_cutout_crowns() {
    let chunks: Vec<_> = (-3..=3)
        .flat_map(|x| (-3..=3).map(move |z| ChunkCoord::new(x, z)))
        .collect();
    let (registries, state) = generated(&chunks);
    let textures = bake(&registries);
    let mesher = SectionMesher::new(&registries, &textures);
    let chunk = first_tree_chunk(&state);
    let origin = VoxelCoord::new(0, 0, 0);
    let summer = mesher.mesh_chunk_trees(
        state.world(),
        chunk,
        tick_in(&registries, Season::Summer),
        origin,
    );
    assert!(!summer.opaque().is_empty(), "trunks are opaque geometry");
    assert!(
        !summer.cutout().is_empty(),
        "summer crowns are alpha-tested foliage"
    );

    let winter = mesher.mesh_chunk_trees(
        state.world(),
        chunk,
        tick_in(&registries, Season::Winter),
        origin,
    );
    let deciduous_only = state
        .world()
        .trees()
        .filter(|tree| ChunkCoord::containing(tree.root()) == chunk)
        .all(|tree| !tree.species().evergreen());
    if deciduous_only {
        assert!(
            winter.cutout().is_empty(),
            "bare deciduous trees keep no winter crown"
        );
    }
    assert!(winter.cutout().vertices().len() <= summer.cutout().vertices().len());
    assert_eq!(
        mesher.mesh_chunk_trees(state.world(), chunk, SimulationTick::new(0), origin),
        mesher.mesh_chunk_trees(state.world(), chunk, SimulationTick::new(0), origin)
    );
    assert_ne!(BlockStateId::AIR, BLOCK_STATE_STONE);
}
