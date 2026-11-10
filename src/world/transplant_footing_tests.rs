//! A player may stand on a trunk, but another rooted tree requires solid terrain.

use super::*;
use crate::content::{BLOCK_STATE_STONE, build_registries};
use crate::world::{BlockStateId, ChunkCoord, WorldGenerationSeed, validate_generate_chunk};

#[test]
fn transplant_destination_cannot_root_on_another_living_tree() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(48_219),
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let lower_tree = state
        .world()
        .trees()
        .next()
        .copied()
        .expect("seeded stone chunk contains a standing tree");
    let trunk_head = VoxelCoord::new(
        lower_tree.root().x(),
        lower_tree.root().y() + 1,
        lower_tree.root().z(),
    );
    let upper_root = VoxelCoord::new(
        lower_tree.root().x(),
        lower_tree.root().y() + 2,
        lower_tree.root().z(),
    );
    assert_eq!(state.world().block_at(trunk_head), Some(BlockStateId::AIR));
    assert!(state.world().tree_trunk_at(trunk_head).is_some());
    assert!(
        state
            .world()
            .has_standing_clearance(registries.world(), upper_root),
        "the existing lower trunk can physically support a player's feet"
    );
    assert!(
        !destination_clear(&registries, &state, upper_root),
        "a transplant cannot root in air above a tree that can later be felled"
    );
}
