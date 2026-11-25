//! Vertical momentum, gravity, fall impacts and voxel-face collision resolution.

use super::{
    AppState, CONTROL_TICK_DURATION, LocomotionDefinition, MICROSECONDS_PER_SECOND,
    NANOMETERS_PER_MICROMETER, PlayerBodyMotionError, PlayerBodyState, PlayerControlAvailability,
    PlayerControlInput, PlayerFallImpactOutcome, Registries, Speed, WorldPoint,
    assess_player_carrying, body_height, horizontal_chunks_generated, horizontal_voxel_bounds,
    integrate_velocity, length_nanometers, player_cell_blocks_motion,
    player_cell_has_fixed_support, player_voxel_change_allowed,
};
use crate::spatial::{NANOMETERS_PER_VOXEL, VerticalVelocity, VoxelCoord};

pub(super) fn resolve_vertical_control(
    registries: &Registries,
    state: &AppState,
    horizontal_body: PlayerBodyState,
    locomotion: LocomotionDefinition,
    availability: PlayerControlAvailability,
    input: PlayerControlInput,
) -> Result<(PlayerBodyState, bool, Option<PlayerFallImpactOutcome>), PlayerBodyMotionError> {
    let horizontal_bounds = horizontal_voxel_bounds(horizontal_body, locomotion)
        .ok_or(PlayerBodyMotionError::CoordinateOverflow)?;
    let velocity_before = jump_or_existing_vertical_velocity(
        state,
        horizontal_body,
        locomotion,
        availability,
        input,
    )?;
    let gravity_delta = gravity_velocity_delta(registries)?;
    let maximum_fall_speed = i64::try_from(locomotion.maximum_fall_speed().nanometers_per_second())
        .map_err(|_| PlayerBodyMotionError::VelocityOverflow)?;
    let vertical_velocity = velocity_before
        .nanometers_per_second()
        .checked_sub(gravity_delta)
        .ok_or(PlayerBodyMotionError::VelocityOverflow)?
        .max(-maximum_fall_speed);
    let proposed_y = horizontal_body
        .foot_position()
        .y_nanometers()
        .checked_add(integrate_velocity(vertical_velocity)?)
        .ok_or(PlayerBodyMotionError::CoordinateOverflow)?;
    let (resolved_y, resolved_velocity, grounded) = resolve_vertical_collision(
        registries,
        state,
        horizontal_body,
        locomotion,
        horizontal_bounds,
        proposed_y,
        vertical_velocity,
    )?;
    let fall_impact = (!availability.grounded && grounded && vertical_velocity < 0).then(|| {
        let impact_speed = Speed::from_nanometers_per_second(vertical_velocity.unsigned_abs());
        PlayerFallImpactOutcome {
            impact_speed,
            vitality_loss_ppm: locomotion.fall_impact_vitality_loss_ppm(impact_speed),
        }
    });
    let position = WorldPoint::new(
        horizontal_body.foot_position().x_nanometers(),
        resolved_y,
        horizontal_body.foot_position().z_nanometers(),
    );
    Ok((
        horizontal_body.with_motion(
            position,
            horizontal_body.horizontal_velocity(),
            VerticalVelocity::new(resolved_velocity),
            horizontal_body.stance(),
        ),
        grounded,
        fall_impact,
    ))
}

fn jump_or_existing_vertical_velocity(
    state: &AppState,
    body: PlayerBodyState,
    locomotion: LocomotionDefinition,
    availability: PlayerControlAvailability,
    input: PlayerControlInput,
) -> Result<VerticalVelocity, PlayerBodyMotionError> {
    if !availability.accepts_ground_control() || !input.jump() {
        return Ok(body.vertical_velocity());
    }
    // Only voluntary launch is gated: gravity must still integrate an airborne body.
    let player = state
        .logistics()
        .player()
        .unwrap_or_else(|| unreachable!("generated body motion requires admitted player"));
    let origin = body.voxel_position();
    let above = VoxelCoord::new(
        origin.x(),
        origin
            .y()
            .checked_add(1)
            .ok_or(PlayerBodyMotionError::CoordinateOverflow)?,
        origin.z(),
    );
    if !player_voxel_change_allowed(state, *player, above) {
        return Ok(body.vertical_velocity());
    }
    let carrying = assess_player_carrying(state)
        .unwrap_or_else(|| unreachable!("generated body motion requires admitted carrying"));
    let jump_speed = locomotion.jump_speed_for_load(carrying.stored(), carrying.capacity());
    Ok(VerticalVelocity::new(
        i64::try_from(jump_speed.nanometers_per_second())
            .map_err(|_| PlayerBodyMotionError::VelocityOverflow)?,
    ))
}

/// Upper bound on the unobstructed discrete jump rise at the real carried load.
/// Per-frame displacement rounds down, so a bound below a ledge height proves
/// the jump cannot clear it. An adequate bound is not a clearance guarantee.
pub fn project_player_jump_rise_bound(
    registries: &Registries,
    state: &AppState,
) -> Option<crate::core::quantity::Length> {
    let load = assess_player_carrying(state)?;
    let locomotion = registries.survival().physiology().locomotion();
    let velocity = i128::from(
        locomotion
            .jump_speed_for_load(load.stored(), load.capacity())
            .nanometers_per_second(),
    );
    let gravity = i128::from(gravity_velocity_delta(registries).ok()?);
    if gravity <= 0 {
        return None;
    }
    // Gravity is applied before each displacement, including the launch frame.
    let frames = velocity.saturating_sub(1) / gravity;
    let sum = frames.checked_mul(velocity)?.checked_sub(
        gravity
            .checked_mul(frames)?
            .checked_mul(frames.checked_add(1)?)?
            / 2,
    )?;
    let micrometers = sum.checked_mul(i128::from(CONTROL_TICK_DURATION.microseconds()))?
        / MICROSECONDS_PER_SECOND
        / NANOMETERS_PER_MICROMETER;
    Some(crate::core::quantity::Length::from_micrometers(
        u64::try_from(micrometers).ok()?,
    ))
}

fn gravity_velocity_delta(registries: &Registries) -> Result<i64, PlayerBodyMotionError> {
    let acceleration = i128::from(registries.core().gravity().micrometers_per_second_squared())
        .checked_mul(NANOMETERS_PER_MICROMETER)
        .ok_or(PlayerBodyMotionError::VelocityOverflow)?;
    let delta = acceleration
        .checked_mul(i128::from(CONTROL_TICK_DURATION.microseconds()))
        .ok_or(PlayerBodyMotionError::VelocityOverflow)?
        / MICROSECONDS_PER_SECOND;
    i64::try_from(delta).map_err(|_| PlayerBodyMotionError::VelocityOverflow)
}

fn resolve_vertical_collision(
    registries: &Registries,
    state: &AppState,
    body: PlayerBodyState,
    locomotion: LocomotionDefinition,
    bounds: (i64, i64, i64, i64),
    proposed_y: i64,
    proposed_velocity: i64,
) -> Result<(i64, i64, bool), PlayerBodyMotionError> {
    let current_y = body.foot_position().y_nanometers();
    if proposed_y <= current_y {
        let floor = crossed_floor_y(
            &|voxel| player_cell_blocks_motion(registries, state, voxel),
            bounds,
            proposed_y,
            current_y,
        )?;
        if let Some(floor_y) = floor {
            return Ok((floor_y, 0, true));
        }
        return Ok((proposed_y, proposed_velocity, false));
    }

    let height = length_nanometers(body_height(locomotion, body.stance()))
        .ok_or(PlayerBodyMotionError::CoordinateOverflow)?;
    let current_top = current_y
        .checked_add(height)
        .ok_or(PlayerBodyMotionError::CoordinateOverflow)?;
    let proposed_top = proposed_y
        .checked_add(height)
        .ok_or(PlayerBodyMotionError::CoordinateOverflow)?;
    if let Some(ceiling) = crossed_ceiling_y(
        &|voxel| player_cell_blocks_motion(registries, state, voxel),
        bounds,
        current_top,
        proposed_top,
    )? {
        let foot = ceiling
            .checked_sub(height)
            .ok_or(PlayerBodyMotionError::CoordinateOverflow)?;
        return Ok((foot, 0, false));
    }
    Ok((proposed_y, proposed_velocity, false))
}

/// Check a short adjacent descent into the center of a neighboring column.
/// The standing-body entrance must be open, and gravity must encounter the
/// requested landing before any intervening floor. This does not guarantee
/// that the actual horizontal approach or a later control sample succeeds.
#[must_use]
pub fn project_player_descent_approach(
    registries: &Registries,
    state: &AppState,
    destination: VoxelCoord,
) -> bool {
    let Some(player) = state.logistics().player() else {
        return false;
    };
    let origin = player.position();
    let depth = i128::from(origin.y()) - i128::from(destination.y());
    if !(1..=3).contains(&depth)
        || i128::from(origin.x().abs_diff(destination.x()))
            + i128::from(origin.z().abs_diff(destination.z()))
            != 1
        || !super::collision::player_body_is_grounded_at(registries, state, player.body())
    {
        return false;
    }
    let entrance = VoxelCoord::new(destination.x(), origin.y(), destination.z());
    let Some(body) = PlayerBodyState::centered_on_voxel_floor(entrance) else {
        return false;
    };
    let locomotion = registries.survival().physiology().locomotion();
    let Some(bounds) = horizontal_voxel_bounds(body, locomotion) else {
        return false;
    };
    let blocked = |voxel| player_cell_blocks_motion(registries, state, voxel);
    if !horizontal_chunks_generated(state.world(), bounds)
        || !super::collision::player_body_fits_continuous_world(body, locomotion)
        || super::collision::first_player_body_collision_where(body, locomotion, blocked).is_some()
    {
        return false;
    }
    let Some(landing) = WorldPoint::centered_on_voxel_floor(destination) else {
        return false;
    };
    let landing_y = landing.y_nanometers();
    crossed_floor_y(
        &blocked,
        bounds,
        landing_y,
        body.foot_position().y_nanometers(),
    ) == Ok(Some(landing_y))
        && super::project_player_standing_space(registries, state, destination)
}

fn crossed_floor_y(
    blocked: &impl Fn(VoxelCoord) -> bool,
    (min_x, max_x, min_z, max_z): (i64, i64, i64, i64),
    proposed_y: i64,
    current_y: i64,
) -> Result<Option<i64>, PlayerBodyMotionError> {
    // The lower face of the body may land exactly on a block's upper face.
    // Include the cell immediately below that plane, just as grounding does,
    // or an exact-face landing is reported as airborne for an extra tick.
    let first_cell = proposed_y.div_euclid(NANOMETERS_PER_VOXEL);
    let min_y = if proposed_y.rem_euclid(NANOMETERS_PER_VOXEL) == 0 {
        first_cell - 1
    } else {
        first_cell
    };
    let max_y = current_y
        .checked_sub(1)
        .ok_or(PlayerBodyMotionError::CoordinateOverflow)?
        .div_euclid(NANOMETERS_PER_VOXEL);
    let mut floor = None;
    for y in min_y..=max_y {
        let top = y
            .checked_add(1)
            .and_then(|value| value.checked_mul(NANOMETERS_PER_VOXEL))
            .ok_or(PlayerBodyMotionError::CoordinateOverflow)?;
        if top < proposed_y || top > current_y {
            continue;
        }
        for z in min_z..=max_z {
            for x in min_x..=max_x {
                if blocked(VoxelCoord::new(x, y, z)) {
                    floor = Some(floor.map_or(top, |existing: i64| existing.max(top)));
                }
            }
        }
    }
    Ok(floor)
}

pub(super) fn crossed_ceiling_y(
    blocked: &impl Fn(VoxelCoord) -> bool,
    (min_x, max_x, min_z, max_z): (i64, i64, i64, i64),
    current_top: i64,
    proposed_top: i64,
) -> Result<Option<i64>, PlayerBodyMotionError> {
    let min_y = current_top.div_euclid(NANOMETERS_PER_VOXEL);
    let max_y = proposed_top
        .checked_sub(1)
        .ok_or(PlayerBodyMotionError::CoordinateOverflow)?
        .div_euclid(NANOMETERS_PER_VOXEL);
    let mut ceiling = None;
    for y in min_y..=max_y {
        let bottom = y
            .checked_mul(NANOMETERS_PER_VOXEL)
            .ok_or(PlayerBodyMotionError::CoordinateOverflow)?;
        if bottom < current_top || bottom >= proposed_top {
            continue;
        }
        for z in min_z..=max_z {
            for x in min_x..=max_x {
                if blocked(VoxelCoord::new(x, y, z)) {
                    ceiling = Some(ceiling.map_or(bottom, |existing: i64| existing.min(bottom)));
                }
            }
        }
    }
    Ok(ceiling)
}

/// Long player-owned attention cannot use a moving temple resident as its
/// sole support. Use the same exact voxel-face and footprint test as gravity,
/// excluding only actor occupancy rather than inventing a second geometry.
#[must_use]
pub fn player_body_has_fixed_floor(registries: &Registries, state: &AppState) -> bool {
    let Some(player) = state.logistics().player() else {
        return false;
    };
    body_has_fixed_floor(registries, state, player.body())
}

fn body_has_fixed_floor(registries: &Registries, state: &AppState, body: PlayerBodyState) -> bool {
    let locomotion = registries.survival().physiology().locomotion();
    let Some(bounds) = horizontal_voxel_bounds(body, locomotion) else {
        return false;
    };
    if !horizontal_chunks_generated(state.world(), bounds) {
        return false;
    }
    let foot = body.foot_position().y_nanometers();
    let Some(probe) = foot.checked_sub(1) else {
        return false;
    };
    crossed_floor_y(
        &|voxel| player_cell_has_fixed_support(registries, state, voxel),
        bounds,
        probe,
        foot,
    )
    .ok()
    .flatten()
        == Some(foot)
}

/// Stationary custody appears in the player's discrete voxel, not across the
/// entire continuous body footprint. A player may stand on the lip of a shaft:
/// the adjacent ledge supports the body, but it cannot support a parcel, tool,
/// cache or machine placed in the empty voxel under the player's center.
///
/// The custody voxel, not the full body footprint, determines whether terrain
/// authority exists. Abstract opening graphs retain their own locality rule.
/// In generated terrain require body contact and the exact stationary floor;
/// a partially generated footprint cannot prove that contact.
pub(crate) fn player_stationary_placement_supported(
    registries: &Registries,
    state: &AppState,
) -> bool {
    let Some(player) = state.logistics().player() else {
        return false;
    };
    !state.world().has_generated_terrain_at(player.position())
        || body_has_fixed_placement_floor(registries, state, player.body())
}

/// Evaluate a proposed body and its exact custody voxel, including respawn
/// candidates which are not yet the current player body.
pub(crate) fn body_has_fixed_placement_floor(
    registries: &Registries,
    state: &AppState,
    body: PlayerBodyState,
) -> bool {
    body_has_fixed_floor(registries, state, body)
        && !crate::world::generated_position_lacks_fixed_footing(
            registries,
            state,
            body.voxel_position(),
        )
}

pub(super) fn supporting_floor_y(
    registries: &Registries,
    state: &AppState,
    body: PlayerBodyState,
    bounds: (i64, i64, i64, i64),
) -> Option<i64> {
    let foot = body.foot_position().y_nanometers();
    let probe = foot.checked_sub(1)?;
    crossed_floor_y(
        &|voxel| player_cell_blocks_motion(registries, state, voxel),
        bounds,
        probe,
        foot,
    )
    .ok()
    .flatten()
}

#[cfg(test)]
#[path = "vertical_tests.rs"]
mod tests;
