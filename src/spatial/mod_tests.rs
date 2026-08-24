//! Tests for the sibling mod module; isolated so test-only edits do not invalidate production builds.

use super::*;

#[test]
fn checked_offset_rejects_coordinate_wraparound() {
    let origin = VoxelCoord::new(i64::MAX, 0, 0);

    assert_eq!(origin.checked_offset(VoxelDelta::new(1, 0, 0)), None);
    assert_eq!(
        VoxelCoord::new(10, 20, 30).checked_offset(VoxelDelta::new(-2, 3, -4)),
        Some(VoxelCoord::new(8, 23, 26))
    );
}

#[test]
fn bounds_are_half_open_and_count_voxels_exactly() {
    let bounds = match VoxelBounds::new(VoxelCoord::new(-2, 10, 5), VoxelCoord::new(3, 12, 9)) {
        Ok(bounds) => bounds,
        Err(error) => panic!("bounds fixture failed: {error}"),
    };

    assert!(bounds.has_voxel(VoxelCoord::new(-2, 10, 5)));
    assert!(bounds.has_voxel(VoxelCoord::new(2, 11, 8)));
    assert!(!bounds.has_voxel(VoxelCoord::new(3, 11, 8)));
    assert_eq!(bounds.voxel_count(), Some(40));
}

#[test]
fn bounds_intersection_respects_half_open_faces() {
    let left = match VoxelBounds::new(VoxelCoord::new(0, 0, 0), VoxelCoord::new(4, 4, 4)) {
        Ok(bounds) => bounds,
        Err(error) => panic!("left intersection fixture failed: {error}"),
    };
    let overlapping = match VoxelBounds::new(VoxelCoord::new(3, 1, 1), VoxelCoord::new(6, 2, 2)) {
        Ok(bounds) => bounds,
        Err(error) => panic!("overlap intersection fixture failed: {error}"),
    };
    let touching = match VoxelBounds::new(VoxelCoord::new(4, 0, 0), VoxelCoord::new(8, 4, 4)) {
        Ok(bounds) => bounds,
        Err(error) => panic!("touching intersection fixture failed: {error}"),
    };
    let corner_touching = match VoxelBounds::new(VoxelCoord::new(4, 4, 4), VoxelCoord::new(5, 5, 5))
    {
        Ok(bounds) => bounds,
        Err(error) => panic!("corner-touching fixture failed: {error}"),
    };
    let separated = match VoxelBounds::new(VoxelCoord::new(5, 0, 0), VoxelCoord::new(8, 4, 4)) {
        Ok(bounds) => bounds,
        Err(error) => panic!("separated fixture failed: {error}"),
    };

    assert!(left.has_intersection(overlapping));
    assert!(overlapping.has_intersection(left));
    assert_eq!(
        left.intersection(overlapping),
        VoxelBounds::new(VoxelCoord::new(3, 1, 1), VoxelCoord::new(4, 2, 2)).ok()
    );
    assert!(!left.has_intersection(touching));
    assert!(!touching.has_intersection(left));
    assert_eq!(left.intersection(touching), None);
    assert!(left.has_contact(overlapping));
    assert!(left.has_contact(touching));
    assert!(left.has_contact(corner_touching));
    assert!(!left.has_contact(separated));
}

#[test]
fn bounds_reject_zero_or_negative_extent_per_axis() {
    assert_eq!(
        VoxelBounds::new(VoxelCoord::new(0, 0, 0), VoxelCoord::new(0, 1, 1)),
        Err(VoxelBoundsError::NonPositiveExtent { axis: Axis::X })
    );
    assert_eq!(
        VoxelBounds::new(VoxelCoord::new(0, 2, 0), VoxelCoord::new(1, 1, 1)),
        Err(VoxelBoundsError::NonPositiveExtent { axis: Axis::Y })
    );
}

#[test]
fn column_projection_discards_only_vertical_coordinate() {
    assert_eq!(
        ColumnCoord::from(VoxelCoord::new(12, -400, 91)),
        ColumnCoord::new(12, 91)
    );
}
