//! Exact wall-face contact must be symmetric along X/Z and both signs, with no tunneling.

use super::*;
use crate::content::build_registries;

#[test]
fn checked_horizontal_sweep_stops_at_the_first_wall_on_every_cardinal_axis() {
    let registries = build_registries();
    let locomotion = registries.survival().physiology().locomotion();
    let body = PlayerBodyState::centered_on_voxel_floor(VoxelCoord::new(0, 0, 0)).unwrap();
    let (lower, upper) = body_horizontal_extents(locomotion).unwrap();
    let origin = body.foot_position();
    for axis in [HorizontalAxis::X, HorizontalAxis::Z] {
        let current = match axis {
            HorizontalAxis::X => origin.x_nanometers(),
            HorizontalAxis::Z => origin.z_nanometers(),
        };
        for (sign, wall_cell) in [(1_i64, 2_i64), (-1, -2)] {
            let proposed = current + sign * 4 * NANOMETERS_PER_VOXEL;
            let wall = |voxel: VoxelCoord| {
                voxel.y() == 0
                    && match axis {
                        HorizontalAxis::X => voxel.x() == wall_cell && voxel.z() == 0,
                        HorizontalAxis::Z => voxel.z() == wall_cell && voxel.x() == 0,
                    }
            };
            let (resolved, collided) =
                sweep_horizontal_axis(body, locomotion, proposed, axis, &wall).unwrap();
            assert!(collided, "sweep missed the first wall on the {sign} axis");
            let expected = if sign > 0 {
                wall_cell * NANOMETERS_PER_VOXEL - upper
            } else {
                (wall_cell + 1) * NANOMETERS_PER_VOXEL + lower
            };
            assert_eq!(
                resolved, expected,
                "body must stop flush with the contacted face"
            );
            assert_eq!(
                sweep_horizontal_axis(body, locomotion, proposed, axis, &|_| false).unwrap(),
                (proposed, false),
                "open space must not clip motion"
            );
        }
    }
}
