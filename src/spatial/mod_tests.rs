//! Contract tests for persistent spatial coordinates and bounds.

use super::*;

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
fn bounds_deserialization_replays_nonempty_extent_validation() {
    let zero_x = r#"{
        "min":{"x":0,"y":0,"z":0},
        "max_exclusive":{"x":0,"y":1,"z":1}
    }"#;
    assert!(
        serde_json::from_str::<VoxelBounds>(zero_x).is_err(),
        "deserialization must not construct zero-width voxel bounds"
    );

    let inverted_y = r#"{
        "min":{"x":0,"y":2,"z":0},
        "max_exclusive":{"x":1,"y":1,"z":1}
    }"#;
    assert!(
        serde_json::from_str::<VoxelBounds>(inverted_y).is_err(),
        "deserialization must not construct inverted voxel bounds"
    );
}
