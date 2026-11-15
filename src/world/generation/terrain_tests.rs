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

#[test]
fn independent_seeds_create_real_valleys_and_hills_without_chunk_edge_cliffs() {
    let mut regional_signatures = std::collections::BTreeSet::new();
    for number in 0..16 {
        let seed = WorldGenerationSeed::new(number);
        let mut lowest = i64::MAX;
        let mut highest = i64::MIN;
        let mut heights = Vec::new();
        for z in (-192..=192).step_by(8) {
            for x in (-192..=192).step_by(8) {
                let height = terrain_height(seed, x, z);
                lowest = lowest.min(height);
                highest = highest.max(height);
                assert!((-25..=25).contains(&height));
                heights.push(height);
            }
        }
        assert!(
            highest - lowest >= 24,
            "seed {number} lacks meaningful regional travel relief: {lowest}..{highest}"
        );
        regional_signatures.insert((lowest, highest, heights));
        for x in [
            -65, -64, -33, -32, -17, -16, -9, -8, -1, 0, 7, 8, 15, 16, 31, 32, 63, 64,
        ] {
            for z in [-65, -64, -33, -32, -17, -16, -1, 0, 15, 16, 31, 32, 63, 64] {
                let here = terrain_height(seed, x, z);
                assert!(
                    (here - terrain_height(seed, x + 1, z)).abs() <= 4
                        && (here - terrain_height(seed, x, z + 1)).abs() <= 4,
                    "seed {number} has an artificial discontinuity at ({x},{z})"
                );
            }
        }
    }
    assert_eq!(
        regional_signatures.len(),
        16,
        "different seeds must change the topology, not just terrain orientation"
    );
}
