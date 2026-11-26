//! Viewer startup and physical mesh extent contracts.

use super::*;

#[test]
fn preview_uses_the_natural_start_and_bounds_every_rendered_tree_vertex() {
    let options = Options {
        seed: 4_909_227_005_684_505_656,
        radius: 1,
        ..Options::default()
    };
    let registries = build_registries();
    let (initial, spawn) =
        generate_natural_world_start(&registries, WorldGenerationSeed::new(options.seed), None)
            .unwrap();
    let scene = Scene::generate(&options).unwrap();
    assert_eq!(scene.origin, spawn);
    let chunk = ChunkCoord::containing(spawn);
    assert_eq!(
        scene.state.world().get_chunk(chunk),
        initial.world().get_chunk(chunk)
    );
    assert!(scene.state.logistics().player().is_none());
    assert_eq!(
        scene.spawn_eye()[1],
        registries
            .survival()
            .physiology()
            .locomotion()
            .standing_eye_height()
            .micrometers() as f32
            / 1_000_000.0
    );
    assert!(!scene.trees.is_empty());
    for placed in &scene.trees {
        let vertices = placed
            .mesh
            .opaque()
            .vertices()
            .iter()
            .chain(placed.mesh.cutout().vertices());
        for vertex in vertices.clone() {
            let position = vertex.position();
            for (axis, coordinate) in position.into_iter().enumerate() {
                assert!((placed.min[axis]..=placed.max[axis]).contains(&coordinate));
            }
        }
        for axis in 0..3 {
            assert!(
                vertices
                    .clone()
                    .any(|vertex| vertex.position()[axis] == placed.min[axis])
            );
            assert!(
                vertices
                    .clone()
                    .any(|vertex| vertex.position()[axis] == placed.max[axis])
            );
        }
    }
}

#[test]
fn empty_mesh_has_no_render_bounds() {
    assert_eq!(mesh_bounds(&SectionMesh::default()), None);
}
