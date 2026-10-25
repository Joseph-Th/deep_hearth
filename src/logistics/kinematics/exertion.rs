//! Shared physically realized locomotion effort for control ticks and read-only walk plans.

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::CONTROL_TICK_DURATION;
use crate::spatial::{HorizontalVelocity, WorldPoint};
use crate::survival::{LocomotionDefinition, SurvivalExertion};

use super::{
    LocomotionExertionAccumulator, MICROSECONDS_PER_SECOND, PlayerBodyMotionError, PlayerBodyState,
    PlayerControlAvailability, PlayerControlInput, assess_player_carrying, integer_sqrt,
};

pub(super) fn resolve_control_exertion(
    state: &AppState,
    before: PlayerBodyState,
    after: PlayerBodyState,
    locomotion: LocomotionDefinition,
    availability: PlayerControlAvailability,
    input: PlayerControlInput,
    settle_pending_exertion: bool,
) -> Result<(LocomotionExertionAccumulator, SurvivalExertion), PlayerBodyMotionError> {
    let player = state
        .logistics()
        .player()
        .copied()
        .unwrap_or_else(|| unreachable!("generated body motion requires admitted player"));
    let before_accumulator = player.locomotion_exertion();
    let settled = if settle_pending_exertion {
        before_accumulator.pending()
    } else {
        SurvivalExertion::REST
    };
    let mut accumulator = if settle_pending_exertion {
        before_accumulator.settled()
    } else {
        before_accumulator
    };
    if !availability.accepts_ground_control() {
        return Ok((accumulator, settled));
    }
    let walked = !input.horizontal().is_neutral()
        && horizontal_position_changed(before.foot_position(), after.foot_position());
    let launched = input.jump()
        && after.foot_position().y_nanometers() > before.foot_position().y_nanometers();
    if !walked && !launched {
        return Ok((accumulator, settled));
    }
    // Resolve the equipped and carried load only for a physical effort. An idle actor
    // must not recalculate every carried equipment mass at 20 Hz.
    let carrying = assess_player_carrying(state)
        .unwrap_or_else(|| unreachable!("generated body motion requires admitted carrying"));
    if walked {
        let horizontal_exertion = walking_exertion(
            locomotion,
            before.foot_position(),
            after.foot_position(),
            carrying.stored(),
            carrying.capacity(),
        )?
        .unwrap_or_else(|| unreachable!("walk input moved the player's horizontal body"));
        accumulator = accumulator
            .checked_accrue(horizontal_exertion)
            .ok_or(PlayerBodyMotionError::ExertionOverflow)?;
    }
    if launched {
        let jump_exertion =
            locomotion.jump_exertion_for_load(carrying.stored(), carrying.capacity());
        accumulator = accumulator
            .checked_accrue_impulse(jump_exertion)
            .ok_or(PlayerBodyMotionError::ExertionOverflow)?;
    }
    Ok((accumulator, settled))
}

/// Both live control and future-load path projections price the same realized displacement.
/// Neutral-input momentum is not active walking; the caller excludes it.
pub(super) fn walking_exertion(
    locomotion: LocomotionDefinition,
    before: WorldPoint,
    after: WorldPoint,
    carried: Mass,
    capacity: Mass,
) -> Result<Option<SurvivalExertion>, PlayerBodyMotionError> {
    if !horizontal_position_changed(before, after) {
        return Ok(None);
    }
    let velocity = realized_horizontal_velocity(before, after)?;
    let speed = horizontal_speed(velocity)?;
    locomotion
        .exertion_for_horizontal_speed(carried, capacity, speed)
        .map(Some)
        .ok_or(PlayerBodyMotionError::ExertionOverflow)
}

fn horizontal_position_changed(before: WorldPoint, after: WorldPoint) -> bool {
    before.x_nanometers() != after.x_nanometers() || before.z_nanometers() != after.z_nanometers()
}

fn realized_horizontal_velocity(
    before: WorldPoint,
    after: WorldPoint,
) -> Result<HorizontalVelocity, PlayerBodyMotionError> {
    let component = |before: i64, after: i64| {
        i128::from(after)
            .checked_sub(i128::from(before))
            .and_then(|displacement| displacement.checked_mul(MICROSECONDS_PER_SECOND))
            .and_then(|numerator| {
                numerator.checked_div(i128::from(CONTROL_TICK_DURATION.microseconds()))
            })
            .and_then(|velocity| i64::try_from(velocity).ok())
            .ok_or(PlayerBodyMotionError::VelocityOverflow)
    };
    Ok(HorizontalVelocity::new(
        component(before.x_nanometers(), after.x_nanometers())?,
        component(before.z_nanometers(), after.z_nanometers())?,
    ))
}

fn horizontal_speed(velocity: HorizontalVelocity) -> Result<u128, PlayerBodyMotionError> {
    let x = u128::from(velocity.x_nanometers_per_second().unsigned_abs());
    let z = u128::from(velocity.z_nanometers_per_second().unsigned_abs());
    let speed_squared = x
        .checked_mul(x)
        .and_then(|x_squared| {
            z.checked_mul(z)
                .and_then(|z_squared| x_squared.checked_add(z_squared))
        })
        .ok_or(PlayerBodyMotionError::ExertionOverflow)?;
    Ok(integer_sqrt(speed_squared))
}
