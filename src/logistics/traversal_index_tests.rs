//! Local graph routing must not scale with distant edge history or enter persistence.

use super::*;
use crate::content::build_registries;
use crate::core::state::{StateValidationError, validate_loaded_state};
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};

#[test]
fn distant_opening_edges_do_not_change_local_neighbors_or_save_replay() {
    let registries = build_registries();
    let mut state = AppState::new();
    let origin = VoxelCoord::new(0, 0, 0);
    let neighbors = [
        VoxelCoord::new(0, 0, 1),
        VoxelCoord::new(1, 0, 0),
        VoxelCoord::new(-1, 0, 0),
        VoxelCoord::new(0, 1, 0),
        VoxelCoord::new(0, 0, -1),
        VoxelCoord::new(0, -1, 0),
    ];
    for neighbor in neighbors {
        add_traversable_edge_for_test(&mut state, origin, neighbor);
    }
    for n in 0..512_i64 {
        let remote = VoxelCoord::new(10_000 + n * 3, 0, 0);
        add_traversable_edge_for_test(&mut state, remote, VoxelCoord::new(remote.x() + 1, 0, 0));
    }
    let mut sorted = neighbors.to_vec();
    sorted.sort_unstable();
    assert_eq!(
        state
            .logistics()
            .traversable_destinations(origin)
            .collect::<Vec<_>>(),
        sorted
    );
    assert!(state.logistics().has_valid_traversal_index());
    assert_eq!(
        state
            .logistics()
            .traversable_destinations(VoxelCoord::new(8_000, 0, 0))
            .count(),
        0
    );
    assert!(state.logistics().has_traversal_edge(origin, neighbors[0]));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("traversal_neighbors"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&bytes).unwrap();
    let loaded = decoded.into_state(&registries).unwrap();
    assert_eq!(loaded, state);
    assert_eq!(
        loaded
            .logistics()
            .traversable_destinations(origin)
            .collect::<Vec<_>>(),
        sorted
    );
}

#[test]
fn invalid_derived_adjacency_is_rejected_and_reconstructed_from_authoritative_edges() {
    let registries = build_registries();
    let mut state = AppState::new();
    let left = VoxelCoord::new(0, 0, 0);
    let right = VoxelCoord::new(1, 0, 0);
    add_traversable_edge_for_test(&mut state, left, right);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    assert!(
        state
            .logistics_state_mut()
            .traversal_neighbors
            .get_mut(&left)
            .unwrap()
            .remove(&right)
    );
    assert_eq!(
        validate_loaded_state(&registries, &state),
        Err(StateValidationError::Logistics(
            crate::logistics::LogisticsValidationError::TraversalAdjacencyIndexInconsistent,
        ))
    );

    let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&bytes).unwrap();
    let loaded = decoded.into_state(&registries).unwrap();
    assert!(loaded.logistics().has_valid_traversal_index());
    assert_eq!(
        loaded
            .logistics()
            .traversable_destinations(left)
            .collect::<Vec<_>>(),
        vec![right]
    );
    assert_eq!(
        loaded
            .logistics()
            .traversable_destinations(right)
            .collect::<Vec<_>>(),
        vec![left]
    );
}

#[test]
fn generated_opening_transaction_builds_adjacent_routes_without_extra_persistent_custody() {
    let mut logistics = LogisticsState::new();
    let home = VoxelCoord::new(0, 0, 0);
    let north = VoxelCoord::new(0, 0, 1);
    let east = VoxelCoord::new(1, 0, 0);
    let mut edges = vec![
        TraversalEdge::new(home, east).unwrap(),
        TraversalEdge::new(home, north).unwrap(),
    ];
    edges.sort_unstable();
    logistics.apply_generated_opening_access(0, 1, crate::fluid::FluidStoreId::new(1), home, edges);
    assert!(logistics.has_valid_traversal_index());
    assert_eq!(
        logistics.traversable_destinations(home).collect::<Vec<_>>(),
        vec![north, east]
    );
    assert_eq!(
        logistics.traversable_destinations(east).collect::<Vec<_>>(),
        vec![home]
    );
}
