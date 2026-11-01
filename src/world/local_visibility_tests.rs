//! Contracts for bounded visual neighborhood geometry at coordinate extremes.

use std::collections::BTreeSet;

use super::*;

#[test]
fn local_observation_envelope_is_unique_bounded_and_checked_at_world_limits() {
    let center = VoxelCoord::new(0, 0, 0);
    let cells = nearby_two_step_cells(center).collect::<Vec<_>>();
    assert_eq!(cells.len(), 25);
    assert!(cells.contains(&center));
    assert_eq!(cells.iter().copied().collect::<BTreeSet<_>>().len(), 25);
    assert!(
        cells
            .iter()
            .all(|cell| within_manhattan_reach(center, *cell, 2))
    );
    for limit in [i64::MIN, i64::MAX] {
        let edge = VoxelCoord::new(limit, limit, limit);
        let near_edge = nearby_two_step_cells(edge).collect::<Vec<_>>();
        assert_eq!(near_edge.len(), 10);
        assert!(
            near_edge
                .iter()
                .all(|cell| within_manhattan_reach(edge, *cell, 2))
        );
    }
}
