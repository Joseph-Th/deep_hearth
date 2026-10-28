//! Spatially restricted world queries preserve exact chunk-order collision truth.

use super::*;
use crate::content::{BLOCK_STATE_STONE, build_registries};
use crate::core::state::AppState;
use crate::spatial::{VoxelBounds, VoxelCoord};

#[test]
fn horizontal_chunk_range_selects_only_overlapping_columns_in_canonical_order() {
    let registries = build_registries();
    let seed = WorldGenerationSeed::new(41);
    let mut state = AppState::new();
    // Deliberately insert out of order, with irrelevant columns sharing x or z.
    for coord in [
        ChunkCoord::new(2, 0),
        ChunkCoord::new(-1, 2),
        ChunkCoord::new(-2, 0),
        ChunkCoord::new(0, 3),
        ChunkCoord::new(-1, -1),
        ChunkCoord::new(1, 2),
        ChunkCoord::new(0, 0),
    ] {
        validate_generate_chunk(&registries, &state, seed, coord, BLOCK_STATE_STONE)
            .unwrap()
            .commit(&mut state)
            .unwrap();
    }
    for (min, max, expected) in [
        (
            VoxelCoord::new(-20, -64, -2),
            VoxelCoord::new(1, 320, 1),
            vec![
                ChunkCoord::new(-2, 0),
                ChunkCoord::new(-1, -1),
                ChunkCoord::new(0, 0),
            ],
        ),
        (
            VoxelCoord::new(0, 0, 0),
            VoxelCoord::new(16, 1, 16),
            vec![ChunkCoord::new(0, 0)],
        ),
        (
            VoxelCoord::new(16, 0, 16),
            VoxelCoord::new(32, 1, 48),
            vec![ChunkCoord::new(1, 2)],
        ),
        (
            VoxelCoord::new(-64, 0, 48),
            VoxelCoord::new(-48, 1, 64),
            vec![],
        ),
    ] {
        let bounds = VoxelBounds::new(min, max).unwrap();
        let chosen: Vec<_> = state
            .world()
            .chunks_intersecting_horizontal_bounds(bounds)
            .map(WorldChunk::coord)
            .collect();
        assert_eq!(chosen, expected, "wrong columns for {bounds:?}");
    }
}

#[test]
fn structural_clearance_uses_only_existing_world_matter_in_candidate_columns() {
    let registries = build_registries();
    let seed = WorldGenerationSeed::new(41);
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        seed,
        ChunkCoord::new(-1, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let empty = VoxelBounds::new(VoxelCoord::new(0, 0, 0), VoxelCoord::new(1, 1, 1)).unwrap();
    assert_eq!(
        first_solid_block_in_structural_bounds(&registries, &state, empty),
        None
    );
    let solid = VoxelCoord::new(-1, -63, 0);
    let occupied = VoxelBounds::new(
        solid,
        VoxelCoord::new(solid.x() + 1, solid.y() + 1, solid.z() + 1),
    )
    .unwrap();
    assert_eq!(
        first_solid_block_in_structural_bounds(&registries, &state, occupied),
        Some(solid)
    );
}
