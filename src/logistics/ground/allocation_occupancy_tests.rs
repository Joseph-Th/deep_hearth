//! Empty ground allocation cannot reserve a standing tree's physical trunk.

use super::*;
use crate::content::{BLOCK_STATE_STONE, build_registries};
use crate::core::state::validate_loaded_state;
use crate::world::{ChunkCoord, WorldGenerationSeed, validate_generate_chunk};

fn generated_tree() -> (crate::registry::Registries, AppState, VoxelCoord) {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(41),
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let trunk = state.world().trees().next().unwrap().root();
    assert!(state.world().tree_trunk_at(trunk).is_some());
    (registries, state, trunk)
}

#[test]
fn empty_ground_allocation_cannot_overlap_a_living_trunk() {
    let (registries, state, trunk) = generated_tree();
    let before = state.clone();
    for at in [trunk, VoxelCoord::new(trunk.x(), trunk.y() + 1, trunk.z())] {
        assert!(state.world().tree_trunk_at(at).is_some());
        assert_eq!(
            validate_allocate_ground_stockpile(&state, at, Mass::from_milligrams(100)).err(),
            Some(GroundStockpileAllocationError::StandingTreeOccupied { position: at })
        );
    }
    assert_eq!(state, before);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn nonphysical_ground_allocation_still_allows_empty_markers_in_generated_rock() {
    use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};

    let (registries, mut state, trunk) = generated_tree();
    let at = VoxelCoord::new(trunk.x(), crate::world::WORLD_MIN_Y, trunk.z());
    assert_eq!(state.world().block_at(at), Some(BLOCK_STATE_STONE));
    let allocation =
        validate_allocate_ground_stockpile(&state, at, Mass::from_milligrams(100)).unwrap();
    let stockpile = allocation.commit(&mut state).unwrap();
    assert_eq!(
        state.logistics().stationary_stockpile_position(stockpile),
        Some(at)
    );
    assert!(
        state
            .inventory()
            .get_stockpile(stockpile)
            .unwrap()
            .stored_mass()
            .is_zero()
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    assert_eq!(
        serde_json::from_slice::<LoadedSaveEnvelope>(&bytes)
            .unwrap()
            .into_state(&registries)
            .unwrap(),
        state
    );
}

#[test]
fn ground_allocation_rechecks_tree_occupancy_when_world_changes_before_commit() {
    let (registries, _generated, trunk) = generated_tree();
    let mut state = AppState::new();
    let allocation =
        validate_allocate_ground_stockpile(&state, trunk, Mass::from_milligrams(100)).unwrap();
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(41),
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    assert!(state.world().tree_trunk_at(trunk).is_some());
    let before = state.clone();
    assert_eq!(
        allocation.commit(&mut state),
        Err(GroundStockpileAllocationCommitError::StandingTreeOccupied { position: trunk })
    );
    assert_eq!(state, before);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
