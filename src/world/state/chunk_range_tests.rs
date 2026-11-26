//! Exact world range selection and generated authority on sparse chunk vectors.

use super::*;
use crate::spatial::VoxelBounds;

#[test]
fn chunk_ranges_match_canonical_filter_and_exact_authority() {
    let mut world = WorldState::new();
    for x in -3..=3 {
        for z in -3..=3 {
            if (x + 2 * z) % 5 != 0 {
                world.chunks.push(WorldChunk::new(ChunkCoord::new(x, z)));
            }
        }
    }
    for coord in [
        ChunkCoord::containing(VoxelCoord::new(i64::MIN, 0, i64::MIN)),
        ChunkCoord::containing(VoxelCoord::new(i64::MAX, 0, i64::MAX)),
        ChunkCoord::new(0, -20_000),
        ChunkCoord::new(0, 20_000),
    ] {
        world.chunks.push(WorldChunk::new(coord));
    }
    world.chunks.sort_unstable_by_key(WorldChunk::coord);
    let compare = |bounds: VoxelBounds| {
        let expected = world
            .chunks()
            .filter(|chunk| {
                let coord = chunk.coord();
                let x = i128::from(coord.x()) * CHUNK_SIDE as i128;
                let z = i128::from(coord.z()) * CHUNK_SIDE as i128;
                x < i128::from(bounds.max_exclusive().x())
                    && x + CHUNK_SIDE as i128 > i128::from(bounds.min().x())
                    && z < i128::from(bounds.max_exclusive().z())
                    && z + CHUNK_SIDE as i128 > i128::from(bounds.min().z())
            })
            .map(WorldChunk::coord)
            .collect::<Vec<_>>();
        assert_eq!(
            world
                .chunks_intersecting_horizontal_bounds(bounds)
                .map(WorldChunk::coord)
                .collect::<Vec<_>>(),
            expected,
            "wrong ordered selection for {bounds:?}"
        );
    };
    let edges = [-65, -48, -17, -16, -1, 0, 1, 15, 16, 17, 48, 65];
    for (i, &x0) in edges.iter().enumerate() {
        for &x1 in &edges[i + 1..] {
            for (j, &z0) in edges.iter().enumerate() {
                for &z1 in &edges[j + 1..] {
                    let bounds =
                        VoxelBounds::new(VoxelCoord::new(x0, 0, z0), VoxelCoord::new(x1, 1, z1))
                            .unwrap();
                    compare(bounds);
                    // Reference proof enumerates only the deliberately small fixture bounds.
                    let first = ChunkCoord::containing(bounds.min());
                    let last = ChunkCoord::containing(VoxelCoord::new(x1 - 1, 0, z1 - 1));
                    let complete = (first.x()..=last.x()).all(|x| {
                        (first.z()..=last.z()).all(|z| {
                            world
                                .chunks()
                                .any(|chunk| chunk.coord() == ChunkCoord::new(x, z))
                        })
                    });
                    assert_eq!(world.has_generated_bounds(bounds), complete);
                }
            }
        }
    }
    for (min, max, complete) in [
        (
            VoxelCoord::new(i64::MIN, WORLD_MIN_Y, i64::MIN),
            VoxelCoord::new(i64::MIN + 1, WORLD_MAX_Y_EXCLUSIVE, i64::MIN + 1),
            true,
        ),
        (
            VoxelCoord::new(i64::MAX - 1, 0, i64::MAX - 1),
            VoxelCoord::new(i64::MAX, 1, i64::MAX),
            true,
        ),
        (
            VoxelCoord::new(i64::MIN, 0, i64::MIN),
            VoxelCoord::new(i64::MAX, 1, i64::MAX),
            false,
        ),
        (
            VoxelCoord::new(i64::MIN, WORLD_MIN_Y - 1, i64::MIN),
            VoxelCoord::new(i64::MIN + 1, WORLD_MIN_Y, i64::MIN + 1),
            false,
        ),
    ] {
        let bounds = VoxelBounds::new(min, max).unwrap();
        compare(bounds);
        assert_eq!(world.has_generated_bounds(bounds), complete);
    }
}
