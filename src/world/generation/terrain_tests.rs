//! Cached chunk sampling must match independent point terrain identity at every coordinate.

use super::{terrain_height, terrain_heights_for_chunk};
use crate::world::{CHUNK_SIDE, ChunkCoord, WorldGenerationSeed};

#[test]
fn cached_chunk_heights_match_independent_world_height_at_every_column() {
    for seed in [
        WorldGenerationSeed::new(0),
        WorldGenerationSeed::new(u64::MAX),
        WorldGenerationSeed::new(0x1EAF_BA5E_2026_1042),
    ] {
        for (cx, cz) in [
            (0, 0),
            (1, -1),
            (-1, 1),
            (-100, -101),
            (1001, 1000),
            (i64::MAX / CHUNK_SIDE as i64, i64::MIN / CHUNK_SIDE as i64),
            (i64::MIN / CHUNK_SIDE as i64, i64::MAX / CHUNK_SIDE as i64),
        ] {
            let chunk = ChunkCoord::new(cx, cz);
            let (min_x, min_z) = chunk.bounded_world_origin().unwrap();
            let cached = terrain_heights_for_chunk(seed, min_x, min_z);
            for (z, row) in cached.iter().enumerate() {
                for (x, &height) in row.iter().enumerate() {
                    let world_x = min_x + x as i64;
                    let world_z = min_z + z as i64;
                    assert_eq!(
                        height,
                        terrain_height(seed, world_x, world_z),
                        "seed={seed:?}, chunk={chunk:?}, column=({world_x},{world_z})"
                    );
                }
            }
        }
    }
}
