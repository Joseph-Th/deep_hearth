//! Short ledge ascent through the ordinary continuous-body kernel.
//! A held cardinal climb input supplies effort against a real nearby fixed face.
//! No climb state survives release: the next neutral frame resumes gravity.

use crate::core::quantity::Mass;
use crate::spatial::{
    HorizontalVelocity, NANOMETERS_PER_VOXEL, VerticalVelocity, VoxelCoord, WorldPoint,
};

use super::collision::{first_player_body_collision_where, player_body_is_grounded_at};
use super::horizontal::{HorizontalAxis, player_voxel_change_allowed, sweep_horizontal_axis};
use super::vertical::crossed_ceiling_y;
use super::{
    AppState, HorizontalControlInput, LocomotionDefinition, PlayerBodyMotionError, PlayerBodyState,
    PlayerStance, Registries, assess_player_carrying, body_height, horizontal_voxel_bounds,
    integrate_velocity, length_nanometers, player_cell_blocks_motion,
    player_cell_has_fixed_support, project_player_standing_space,
};
use crate::survival::Vitality;

/// A two-metre ledge is reachable by a deliberate hand-assisted ascent, not an ordinary jump.
/// Do not silently promote this to general climbing up a vertical wall.
pub const MAX_LEDGE_CLIMB_METERS: i64 = 2;
const MAX_GRIP_DISTANCE_NM: i64 = 850_000_000;

#[derive(Clone, Copy)]
struct Ledge {
    destination: VoxelCoord,
    axis: HorizontalAxis,
    axis_end: i64,
    top: i64,
}

/// Read-only short-ledge feasibility from the actual body, including the lift
/// column, the complete horizontal clearance sweep and the fixed landing floor.
/// This discovers only the immediately adjacent cardinal ledge.
pub fn project_player_climb_target(
    registries: &Registries,
    state: &AppState,
    direction: HorizontalControlInput,
) -> Option<VoxelCoord> {
    let body = state.logistics().player()?.body();
    if state.player_work().active().is_some()
        || state
            .survival()
            .player()
            .is_none_or(|player| player.vitality() == Vitality::ZERO)
    {
        return None;
    }
    let locomotion = registries.survival().physiology().locomotion();
    ledge(registries, state, body, locomotion, direction).map(|ledge| ledge.destination)
}

fn ledge(
    registries: &Registries,
    state: &AppState,
    body: PlayerBodyState,
    locomotion: LocomotionDefinition,
    direction: HorizontalControlInput,
) -> Option<Ledge> {
    if body.stance() != PlayerStance::Standing
        || body.vertical_velocity().nanometers_per_second()
            < -(i64::try_from(locomotion.safe_landing_speed().nanometers_per_second()).ok()?)
    {
        return None;
    }
    // Diagonal and neutral input cannot select a unique climb face.
    let (dx, dz) = (
        i64::from(direction.x_ppm().signum()),
        i64::from(direction.z_ppm().signum()),
    );
    if dx.abs() + dz.abs() != 1 {
        return None;
    }
    let origin = body.voxel_position();
    let x = origin.x().checked_add(dx)?;
    let z = origin.z().checked_add(dz)?;
    let foot = body.foot_position();
    let (axis, coordinate, index, sign) = if dx != 0 {
        (HorizontalAxis::X, foot.x_nanometers(), x, dx)
    } else {
        (HorizontalAxis::Z, foot.z_nanometers(), z, dz)
    };
    let axis_end = index
        .checked_mul(NANOMETERS_PER_VOXEL)?
        .checked_add(NANOMETERS_PER_VOXEL / 2)?;
    let face = axis_end.checked_sub(sign * (NANOMETERS_PER_VOXEL / 2))?;
    if coordinate.abs_diff(face) > MAX_GRIP_DISTANCE_NM as u64 {
        return None;
    }
    for rise in 0..=MAX_LEDGE_CLIMB_METERS {
        let destination = VoxelCoord::new(x, origin.y().checked_add(rise)?, z);
        let top = destination.y().checked_mul(NANOMETERS_PER_VOXEL)?;
        let lift = top.checked_sub(foot.y_nanometers())?;
        if !(0..=MAX_LEDGE_CLIMB_METERS * NANOMETERS_PER_VOXEL).contains(&lift) {
            continue;
        }
        let floor = VoxelCoord::new(x, destination.y().checked_sub(1)?, z);
        if !state.world().has_generated_terrain_at(floor)
            || !player_cell_has_fixed_support(registries, state, floor)
            || !project_player_standing_space(registries, state, destination)
        {
            continue;
        }
        // A zero-rise continuation is a hand-supported transfer across the
        // lip, never a way to replace ordinary level walking.
        let origin_floor = VoxelCoord::new(origin.x(), origin.y().checked_sub(1)?, origin.z());
        if lift == 0 && player_cell_has_fixed_support(registries, state, origin_floor) {
            continue;
        }
        // On a two-cell face the lower cell must also be real support.
        // A floating panel overhead is not a climbable rock wall.
        if lift > NANOMETERS_PER_VOXEL
            && !player_cell_has_fixed_support(registries, state, VoxelCoord::new(x, origin.y(), z))
        {
            continue;
        }
        let player = *state.logistics().player()?;
        if !player_voxel_change_allowed(state, player, destination) {
            continue;
        }
        let candidate = Ledge {
            destination,
            axis,
            axis_end,
            top,
        };
        if clear_approach(registries, state, body, locomotion, candidate) {
            return Some(candidate);
        }
    }
    None
}

fn clear_approach(
    registries: &Registries,
    state: &AppState,
    body: PlayerBodyState,
    locomotion: LocomotionDefinition,
    ledge: Ledge,
) -> bool {
    let blocked = |voxel| player_cell_blocks_motion(registries, state, voxel);
    let Some(bounds) = horizontal_voxel_bounds(body, locomotion) else {
        return false;
    };
    let Some(height) = length_nanometers(body_height(locomotion, body.stance())) else {
        return false;
    };
    let foot = body.foot_position();
    let Some((current_top, proposed_top)) = foot
        .y_nanometers()
        .checked_add(height)
        .zip(ledge.top.checked_add(height))
    else {
        return false;
    };
    if !matches!(
        crossed_ceiling_y(&blocked, bounds, current_top, proposed_top),
        Ok(None)
    ) {
        return false;
    }
    let raised = body.with_motion(
        WorldPoint::new(foot.x_nanometers(), ledge.top, foot.z_nanometers()),
        HorizontalVelocity::ZERO,
        VerticalVelocity::ZERO,
        body.stance(),
    );
    if first_player_body_collision_where(raised, locomotion, blocked).is_some()
        || !matches!(
            sweep_horizontal_axis(raised, locomotion, ledge.axis_end, ledge.axis, &blocked),
            Ok((_, false))
        )
    {
        return false;
    }
    let endpoint = match ledge.axis {
        HorizontalAxis::X => WorldPoint::new(ledge.axis_end, ledge.top, foot.z_nanometers()),
        HorizontalAxis::Z => WorldPoint::new(foot.x_nanometers(), ledge.top, ledge.axis_end),
    };
    player_body_is_grounded_at(
        registries,
        state,
        raised.with_motion(
            endpoint,
            HorizontalVelocity::ZERO,
            VerticalVelocity::ZERO,
            raised.stance(),
        ),
    )
}

/// Resolve one sample of a real lift or hand-supported crossing of the lip.
/// Speed is conservative and load-sensitive, using the already-authored
/// crouched locomotion speed. Every sample rechecks grip and clearance.
pub(super) fn resolve_ledge_climb(
    registries: &Registries,
    state: &AppState,
    before: PlayerBodyState,
    locomotion: LocomotionDefinition,
    direction: HorizontalControlInput,
    projected_load: Option<(Mass, Mass)>,
) -> Result<Option<PlayerBodyState>, PlayerBodyMotionError> {
    let Some(ledge) = ledge(registries, state, before, locomotion, direction) else {
        return Ok(None);
    };
    let (carried, capacity) = projected_load.unwrap_or_else(|| {
        let load = assess_player_carrying(state).expect("admitted climbing body has carrying");
        (load.stored(), load.capacity())
    });
    let speed = i64::try_from(
        locomotion
            .crouching_speed_for_load(carried, capacity)
            .nanometers_per_second()
            / 2,
    )
    .map_err(|_| PlayerBodyMotionError::VelocityOverflow)?;
    let stride = integrate_velocity(speed)?;
    if stride <= 0 {
        return Ok(None);
    }
    let foot = before.foot_position();
    let (position, horizontal, vertical) = if foot.y_nanometers() < ledge.top {
        let y = foot
            .y_nanometers()
            .checked_add(stride)
            .ok_or(PlayerBodyMotionError::CoordinateOverflow)?
            .min(ledge.top);
        (
            WorldPoint::new(foot.x_nanometers(), y, foot.z_nanometers()),
            HorizontalVelocity::ZERO,
            VerticalVelocity::new(speed),
        )
    } else {
        let current = match ledge.axis {
            HorizontalAxis::X => foot.x_nanometers(),
            HorizontalAxis::Z => foot.z_nanometers(),
        };
        let difference = ledge
            .axis_end
            .checked_sub(current)
            .ok_or(PlayerBodyMotionError::CoordinateOverflow)?;
        let step = difference.clamp(-stride, stride);
        let end = current
            .checked_add(step)
            .ok_or(PlayerBodyMotionError::CoordinateOverflow)?;
        let velocity = if step == difference {
            0
        } else {
            speed * step.signum()
        };
        match ledge.axis {
            HorizontalAxis::X => (
                WorldPoint::new(end, ledge.top, foot.z_nanometers()),
                HorizontalVelocity::new(velocity, 0),
                VerticalVelocity::ZERO,
            ),
            HorizontalAxis::Z => (
                WorldPoint::new(foot.x_nanometers(), ledge.top, end),
                HorizontalVelocity::new(0, velocity),
                VerticalVelocity::ZERO,
            ),
        }
    };
    let player = *state.logistics().player().expect("admitted climbing body");
    if !player_voxel_change_allowed(state, player, position.containing_voxel()) {
        return Ok(None);
    }
    Ok(Some(before.with_motion(
        position,
        horizontal,
        vertical,
        before.stance(),
    )))
}
