//! Grounded horizontal control, stance, ledge safety, and bounded acceleration.
//! This is the sole horizontal control decision used by live motion and walk replay.

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::CONTROL_TICK_DURATION;
use crate::registry::Registries;
use crate::spatial::{HorizontalVelocity, WorldPoint};
use crate::survival::LocomotionDefinition;

use super::super::{
    CONTROL_INPUT_PARTS_PER_MILLION, HorizontalControlInput, PlayerBodyState, PlayerControlInput,
    PlayerStance, assess_player_carrying,
};
use super::collision::{
    first_player_body_collision_where, horizontal_voxel_bounds, player_cell_blocks_motion,
};
use super::horizontal::{player_voxel_change_allowed, resolve_horizontal_motion};
use super::vertical::supporting_floor_y;
use super::{
    MICROSECONDS_PER_SECOND, NANOMETERS_PER_MICROMETER, PlayerBodyMotionError,
    PlayerControlAvailability,
};

pub(super) fn resolve_horizontal_control(
    registries: &Registries,
    state: &AppState,
    before: PlayerBodyState,
    locomotion: LocomotionDefinition,
    availability: PlayerControlAvailability,
    input: PlayerControlInput,
    projected_load: Option<(Mass, Mass)>,
) -> Result<PlayerBodyState, PlayerBodyMotionError> {
    let desired_stance = if availability.alive && !availability.busy && input.crouch() {
        PlayerStance::Crouching
    } else if availability.alive && !availability.busy {
        PlayerStance::Standing
    } else {
        before.stance()
    };
    let stance = resolve_stance(registries, state, before, locomotion, desired_stance);
    let stance_body = before.with_stance(stance);
    let desired_velocity = resolve_desired_horizontal_velocity(
        state,
        before,
        locomotion,
        stance,
        availability,
        input,
        projected_load,
    )?;
    let mut velocity = if availability.busy {
        HorizontalVelocity::ZERO
    } else {
        accelerate_horizontal_velocity(before.horizontal_velocity(), desired_velocity, locomotion)?
    };
    let (mut position, collided_x, collided_z) =
        resolve_horizontal_motion(stance_body, locomotion, velocity, &|voxel| {
            player_cell_blocks_motion(registries, state, voxel)
        })?;
    velocity = collision_clipped_horizontal_velocity(velocity, collided_x, collided_z);
    (position, velocity) = guard_crouched_ledge(
        registries,
        state,
        stance_body,
        locomotion,
        availability,
        position,
        velocity,
    )?;
    let player =
        state.logistics().player().copied().unwrap_or_else(|| {
            unreachable!("generated-world body motion requires admitted player")
        });
    if position.containing_voxel() != before.voxel_position()
        && !player_voxel_change_allowed(state, player, position.containing_voxel())
    {
        position = before.foot_position();
        velocity = HorizontalVelocity::ZERO;
    }
    Ok(stance_body.with_motion(position, velocity, before.vertical_velocity(), stance))
}

fn resolve_desired_horizontal_velocity(
    state: &AppState,
    before: PlayerBodyState,
    locomotion: LocomotionDefinition,
    stance: PlayerStance,
    availability: PlayerControlAvailability,
    input: PlayerControlInput,
    projected_load: Option<(Mass, Mass)>,
) -> Result<HorizontalVelocity, PlayerBodyMotionError> {
    if !availability.grounded && !availability.busy {
        return Ok(before.horizontal_velocity());
    }
    if !availability.accepts_ground_control() {
        return desired_horizontal_velocity(
            locomotion.walking_speed().nanometers_per_second(),
            HorizontalControlInput::NEUTRAL,
        );
    }
    if input.horizontal().is_neutral() {
        return Ok(HorizontalVelocity::ZERO);
    }
    let (carried, capacity) = projected_load.unwrap_or_else(|| {
        let carrying = assess_player_carrying(state)
            .unwrap_or_else(|| unreachable!("generated body motion requires admitted carrying"));
        (carrying.stored(), carrying.capacity())
    });
    let target_speed = match stance {
        PlayerStance::Crouching => locomotion.crouching_speed_for_load(carried, capacity),
        PlayerStance::Standing if input.sprint() => {
            locomotion.sprinting_speed_for_load(carried, capacity)
        }
        PlayerStance::Standing => locomotion.walking_speed_for_load(carried, capacity),
    };
    desired_horizontal_velocity(target_speed.nanometers_per_second(), input.horizontal())
}

fn collision_clipped_horizontal_velocity(
    velocity: HorizontalVelocity,
    collided_x: bool,
    collided_z: bool,
) -> HorizontalVelocity {
    HorizontalVelocity::new(
        if collided_x {
            0
        } else {
            velocity.x_nanometers_per_second()
        },
        if collided_z {
            0
        } else {
            velocity.z_nanometers_per_second()
        },
    )
}

fn guard_crouched_ledge(
    registries: &Registries,
    state: &AppState,
    stance_body: PlayerBodyState,
    locomotion: LocomotionDefinition,
    availability: PlayerControlAvailability,
    position: WorldPoint,
    velocity: HorizontalVelocity,
) -> Result<(WorldPoint, HorizontalVelocity), PlayerBodyMotionError> {
    if stance_body.stance() != PlayerStance::Crouching || !availability.grounded {
        return Ok((position, velocity));
    }
    let candidate = stance_body.with_motion(
        position,
        velocity,
        stance_body.vertical_velocity(),
        stance_body.stance(),
    );
    let candidate_bounds = horizontal_voxel_bounds(candidate, locomotion)
        .ok_or(PlayerBodyMotionError::CoordinateOverflow)?;
    let support = supporting_floor_y(registries, state, candidate, candidate_bounds);
    if support == Some(stance_body.foot_position().y_nanometers()) {
        Ok((position, velocity))
    } else {
        Ok((stance_body.foot_position(), HorizontalVelocity::ZERO))
    }
}

fn desired_horizontal_velocity(
    speed_nanometers_per_second: u64,
    input: HorizontalControlInput,
) -> Result<HorizontalVelocity, PlayerBodyMotionError> {
    let speed = i128::from(speed_nanometers_per_second);
    let scale = i128::from(CONTROL_INPUT_PARTS_PER_MILLION);
    let component = |ppm: i32| {
        speed
            .checked_mul(i128::from(ppm))
            .and_then(|value| value.checked_div(scale))
            .and_then(|value| i64::try_from(value).ok())
            .ok_or(PlayerBodyMotionError::VelocityOverflow)
    };
    Ok(HorizontalVelocity::new(
        component(input.x_ppm())?,
        component(input.z_ppm())?,
    ))
}

fn resolve_stance(
    registries: &Registries,
    state: &AppState,
    body: PlayerBodyState,
    locomotion: LocomotionDefinition,
    desired: PlayerStance,
) -> PlayerStance {
    if desired == body.stance() || desired == PlayerStance::Crouching {
        return desired;
    }
    let standing = body.with_stance(PlayerStance::Standing);
    if first_player_body_collision_where(standing, locomotion, |voxel| {
        player_cell_blocks_motion(registries, state, voxel)
    })
    .is_none()
    {
        PlayerStance::Standing
    } else {
        PlayerStance::Crouching
    }
}

fn horizontal_acceleration_velocity_delta(
    locomotion: LocomotionDefinition,
) -> Result<i64, PlayerBodyMotionError> {
    let acceleration = i128::from(
        locomotion
            .horizontal_acceleration()
            .micrometers_per_second_squared(),
    )
    .checked_mul(NANOMETERS_PER_MICROMETER)
    .ok_or(PlayerBodyMotionError::VelocityOverflow)?;
    let delta = acceleration
        .checked_mul(i128::from(CONTROL_TICK_DURATION.microseconds()))
        .ok_or(PlayerBodyMotionError::VelocityOverflow)?
        / MICROSECONDS_PER_SECOND;
    i64::try_from(delta).map_err(|_| PlayerBodyMotionError::VelocityOverflow)
}

fn accelerate_horizontal_velocity(
    current: HorizontalVelocity,
    desired: HorizontalVelocity,
    locomotion: LocomotionDefinition,
) -> Result<HorizontalVelocity, PlayerBodyMotionError> {
    let delta_x = i128::from(desired.x_nanometers_per_second())
        - i128::from(current.x_nanometers_per_second());
    let delta_z = i128::from(desired.z_nanometers_per_second())
        - i128::from(current.z_nanometers_per_second());
    let magnitude_squared = delta_x
        .checked_mul(delta_x)
        .and_then(|x| delta_z.checked_mul(delta_z).and_then(|z| x.checked_add(z)))
        .ok_or(PlayerBodyMotionError::VelocityOverflow)?;
    let maximum_delta = i128::from(horizontal_acceleration_velocity_delta(locomotion)?);
    let maximum_squared = maximum_delta
        .checked_mul(maximum_delta)
        .ok_or(PlayerBodyMotionError::VelocityOverflow)?;
    if magnitude_squared <= maximum_squared {
        return Ok(desired);
    }
    let squared =
        u128::try_from(magnitude_squared).map_err(|_| PlayerBodyMotionError::VelocityOverflow)?;
    let floor = squared.isqrt();
    // Rounding the norm down could exceed the authored Euclidean acceleration bound.
    let magnitude = i128::try_from(if floor * floor == squared {
        floor
    } else {
        floor + 1
    })
    .map_err(|_| PlayerBodyMotionError::VelocityOverflow)?;
    if magnitude == 0 {
        return Ok(desired);
    }
    let step_x = delta_x
        .checked_mul(maximum_delta)
        .and_then(|value| value.checked_div(magnitude))
        .ok_or(PlayerBodyMotionError::VelocityOverflow)?;
    let step_z = delta_z
        .checked_mul(maximum_delta)
        .and_then(|value| value.checked_div(magnitude))
        .ok_or(PlayerBodyMotionError::VelocityOverflow)?;
    let x = i128::from(current.x_nanometers_per_second())
        .checked_add(step_x)
        .and_then(|value| i64::try_from(value).ok())
        .ok_or(PlayerBodyMotionError::VelocityOverflow)?;
    let z = i128::from(current.z_nanometers_per_second())
        .checked_add(step_z)
        .and_then(|value| i64::try_from(value).ok())
        .ok_or(PlayerBodyMotionError::VelocityOverflow)?;
    Ok(HorizontalVelocity::new(x, z))
}

#[cfg(test)]
#[path = "acceleration_tests.rs"]
mod acceleration_tests;
