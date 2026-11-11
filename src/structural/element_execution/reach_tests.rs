//! Sub-voxel precision contracts for structural work sight-line targeting.

use super::*;

#[test]
fn near_voxel_corner_ray_retains_unit_direction_precision() {
    let edge = i128::from(NANOMETERS_PER_VOXEL);
    let eye = [edge, edge, edge / 2];
    let target = VoxelCoord::new(1, 1, 0);
    let direction = direction_to_target_interior(eye, target)
        .expect("a one-nanometer edge-crossing diagonal has a normalized direction");
    assert_eq!(direction, ViewDirection::new(707_107, 707_107, 0).unwrap());
}
