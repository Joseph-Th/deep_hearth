//! Exact full-range checks for player-local structural reach geometry.

use super::voxel_is_within_reach;

#[test]
fn structural_reach_preserves_exact_diagonal_checks_near_world_point_limit() {
    let reach = u128::from((i64::MAX as u64) / 1_000) * 1_000;
    assert!(voxel_is_within_reach(
        [0, 0, 0],
        [9_000_000_000, 0, 0],
        reach
    ));
    assert!(!voxel_is_within_reach(
        [0, 0, 0],
        [9_000_000_000, 9_000_000_000, 0],
        reach
    ));
    assert!(!voxel_is_within_reach([0, 0, 0], [i64::MAX, 0, 0], reach));
}
