//! Vertical momentum, gravity, fall impacts and voxel-face collision resolution.

use super::{
    AppState, CONTROL_TICK_DURATION, LocomotionDefinition, MICROSECONDS_PER_SECOND,
    NANOMETERS_PER_MICROMETER, PlayerBodyMotionError, PlayerBodyState, PlayerControlAvailability,
    PlayerControlInput, PlayerFallImpactOutcome, Registries, Speed, WorldPoint,
    assess_player_carrying, body_height, horizontal_voxel_bounds, integrate_velocity,
    length_nanometers, player_cell_blocks_motion,
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
    let carrying = assess_player_carrying(state)
        .unwrap_or_else(|| unreachable!("generated body motion requires admitted carrying"));
    let jump_speed = locomotion.jump_speed_for_load(carrying.stored(), carrying.capacity());
    Ok(VerticalVelocity::new(
        i64::try_from(jump_speed.nanometers_per_second())
            .map_err(|_| PlayerBodyMotionError::VelocityOverflow)?,
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

fn crossed_floor_y(
    blocked: &impl Fn(VoxelCoord) -> bool,
    (min_x, max_x, min_z, max_z): (i64, i64, i64, i64),
    proposed_y: i64,
    current_y: i64,
) -> Result<Option<i64>, PlayerBodyMotionError> {
    let min_y = proposed_y.div_euclid(NANOMETERS_PER_VOXEL);
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

fn crossed_ceiling_y(
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
