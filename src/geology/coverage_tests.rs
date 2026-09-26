//! Behavioral coverage for exact geological region-union geometry.

use super::*;

fn bounds(min_x: i64, max_x: i64) -> VoxelBounds {
    VoxelBounds::new(VoxelCoord::new(min_x, 0, 0), VoxelCoord::new(max_x, 1, 1))
        .unwrap_or_else(|error| panic!("coverage bounds fixture failed: {error}"))
}

#[test]
fn adjacent_bodies_cover_one_region_exactly() {
    let mut coverage = VoxelCoverage::new(bounds(0, 10));
    coverage.cover(bounds(0, 4));
    assert!(!coverage.is_complete());
    coverage.cover(bounds(4, 10));
    assert!(coverage.is_complete());
}

#[test]
fn uncovered_gap_remains_visible() {
    let mut coverage = VoxelCoverage::new(bounds(0, 10));
    coverage.cover(bounds(-5, 4));
    coverage.cover(bounds(5, 15));
    assert!(!coverage.is_complete());
}

#[test]
fn coverage_handles_large_coordinate_span_without_per_voxel_iteration() {
    let mut coverage = VoxelCoverage::new(bounds(-1_000_000_000, 1_000_000_000));
    coverage.cover(bounds(-1_000_000_000, 0));
    coverage.cover(bounds(0, 1_000_000_000));
    assert!(coverage.is_complete());
}
