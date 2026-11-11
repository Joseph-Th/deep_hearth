//! Physical structural-work reach, shared by planning, construction, dismantling and cancellation.
//!
//! Reach is measured from the authoritative stance-dependent eye to the closest
//! point in the target voxel. In generated terrain it also requires an
//! unobstructed eye ray, using the world's existing solid and unknown-cell
//! authority. Ungenerated opening-space scenarios have no terrain to raycast.

use crate::core::state::AppState;
use crate::registry::Registries;
use crate::spatial::{
    NANOMETERS_PER_VOXEL, VIEW_DIRECTION_PARTS_PER_MILLION, ViewDirection, VoxelBounds, VoxelCoord,
};

/// Reach and geometric access to a single player-local structural cell.
pub(crate) fn bounds_are_within_player_reach(
    registries: &Registries,
    state: &AppState,
    bounds: VoxelBounds,
) -> bool {
    if bounds.voxel_count() != Some(1) {
        return false;
    }
    let target = bounds.min();
    if crate::logistics::player_body_occupies_voxel(registries, state, target) {
        return false;
    }
    let Some(player) = state.logistics().player().copied() else {
        return false;
    };
    let locomotion = registries.survival().physiology().locomotion();
    let eye_height = match player.body().stance() {
        crate::logistics::PlayerStance::Standing => locomotion.standing_eye_height(),
        crate::logistics::PlayerStance::Crouching => locomotion.crouching_eye_height(),
    };
    let foot = player.body().foot_position();
    let eye = [
        i128::from(foot.x_nanometers()),
        i128::from(foot.y_nanometers()) + i128::from(eye_height.micrometers()) * 1_000,
        i128::from(foot.z_nanometers()),
    ];
    if !voxel_is_within_reach(
        eye,
        [target.x(), target.y(), target.z()],
        u128::from(locomotion.interaction_reach().micrometers()) * 1_000,
    ) {
        return false;
    }
    if !state.world().has_generated_terrain_at(player.position())
        && !state.world().has_generated_terrain_at(target)
    {
        // Opening-space scenarios have no generated terrain truth to raycast.
        return true;
    }

    // The player cannot work through a terrain wall or a full-cell structure,
    // even when the target is within the interaction radius. The world ray
    // treats the selected structural target as reachable on its exposed face
    // and refuses missing generated truth rather than treating it as air.
    let Some(direction) = direction_to_target_interior(eye, target) else {
        return false;
    };
    // The target's own stone is not an intervening obstruction. Preserve
    // materialization's specific embedded-in-terrain diagnostic, while still
    // rejecting walls that are hit *before* the selected target cell.
    if state.world().block_at(target).is_some_and(|block| {
        registries
            .world()
            .block_for_state(block)
            .is_some_and(|definition| {
                definition.collision() == crate::world::BlockCollisionShape::FullCube
            })
    }) {
        crate::world::raycast_player_block(registries, state, direction)
            .is_ok_and(|hit| hit.is_some_and(|block| block.voxel() == target))
    } else {
        crate::world::raycast_player_air_voxel(registries, state, direction, |voxel, _| {
            (voxel == target).then_some(())
        })
        .is_ok_and(|hit| hit.is_some())
    }
}

/// A ray into the nearest interior point preserves the existing nearest-point
/// reach contract. An arbitrary center ray would reject a valid far edge of a
/// cell when its nearest exposed face remains within hand reach.
fn direction_to_target_interior(eye: [i128; 3], target: VoxelCoord) -> Option<ViewDirection> {
    let mut deltas = [0_i128; 3];
    for (index, coordinate) in [target.x(), target.y(), target.z()].into_iter().enumerate() {
        let minimum = i128::from(coordinate) * i128::from(NANOMETERS_PER_VOXEL);
        let maximum = minimum + i128::from(NANOMETERS_PER_VOXEL);
        let point = eye[index].clamp(minimum + 1, maximum - 1);
        deltas[index] = point - eye[index];
    }
    let squared_length = deltas.into_iter().try_fold(0_u128, |total, delta| {
        let absolute = delta.unsigned_abs();
        total.checked_add(absolute.checked_mul(absolute)?)
    })?;
    let length = squared_length.isqrt();
    if length == 0 {
        return None;
    }
    // Integer square root is accurate at ordinary voxel distances, but its
    // truncation can substantially misnormalize a one-nanometer diagonal
    // near an exact edge. Increase precision only for very short rays.
    let precision = (1_000_000_000_u128 / length).max(1);
    let normalized_length = squared_length
        .checked_mul(precision.checked_mul(precision)?)?
        .isqrt();
    let mut parts = [0_i32; 3];
    for (index, delta) in deltas.into_iter().enumerate() {
        let scaled = delta
            .unsigned_abs()
            .checked_mul(precision)?
            .checked_mul(VIEW_DIRECTION_PARTS_PER_MILLION as u128)?
            .checked_add(normalized_length / 2)?
            / normalized_length;
        let component = i32::try_from(scaled.min(VIEW_DIRECTION_PARTS_PER_MILLION as u128)).ok()?;
        parts[index] = if delta < 0 { -component } else { component };
    }
    ViewDirection::new(parts[0], parts[1], parts[2]).ok()
}

/// Exact shortest point-to-voxel distance, without signed-coordinate overflow.
pub(super) fn voxel_is_within_reach(eye: [i128; 3], voxel: [i64; 3], reach: u128) -> bool {
    // Authored reach fits signed world-point nanometers, so its square fits u128. Subtracting
    // each axis square from the remaining radius budget also avoids intermediate sum overflow.
    let mut remaining_squared = reach * reach;
    for (point, coordinate) in eye.into_iter().zip(voxel) {
        let minimum = i128::from(coordinate) * i128::from(NANOMETERS_PER_VOXEL);
        let maximum = minimum + i128::from(NANOMETERS_PER_VOXEL);
        let signed_distance = if point < minimum {
            minimum - point
        } else if point > maximum {
            point - maximum
        } else {
            0
        };
        let distance = u128::try_from(signed_distance)
            .unwrap_or_else(|_| unreachable!("voxel-to-eye distance is nonnegative"));
        if distance > reach {
            return false;
        }
        let Some(remaining) = remaining_squared.checked_sub(distance * distance) else {
            return false;
        };
        remaining_squared = remaining;
    }
    true
}

#[cfg(test)]
#[path = "reach_tests.rs"]
mod tests;
