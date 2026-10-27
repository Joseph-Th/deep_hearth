//! Deterministic control-rate player-body integration against authoritative block cells.

mod collision;
mod cover;
mod exertion;
mod horizontal;
mod vertical;
mod walk_projection;

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Mass, Speed};
use crate::core::state::AppState;
use crate::core::time::CONTROL_TICK_DURATION;
use crate::registry::Registries;
use crate::spatial::{HorizontalVelocity, WorldPoint};
use crate::survival::{LocomotionDefinition, SurvivalExertion, Vitality};

use super::state::LocomotionExertionAccumulator;
use super::{
    CONTROL_INPUT_PARTS_PER_MILLION, HorizontalControlInput, PlayerBodyState, PlayerControlInput,
    PlayerLogisticsRecord, PlayerStance, assess_player_carrying,
};
use collision::{
    body_height, first_player_body_collision_where, horizontal_chunks_generated,
    horizontal_voxel_bounds, length_nanometers, player_cell_blocks_motion,
};
pub(crate) use collision::{
    first_player_body_collision, first_player_body_collision_in_chunk,
    first_player_body_overlap_with_bounds, first_player_structure_collision,
    first_player_temple_caste_collision, player_body_fits_continuous_world,
    player_body_is_grounded, player_body_occupies_voxel, player_body_stands_above_structure,
    player_body_uses_generated_world,
};
use exertion::resolve_control_exertion;
use horizontal::{player_voxel_change_allowed, resolve_horizontal_motion};
use vertical::{resolve_vertical_control, supporting_floor_y};

const MICROSECONDS_PER_SECOND: i128 = 1_000_000;
const NANOMETERS_PER_MICROMETER: i128 = 1_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerBodyMotionError {
    CoordinateOverflow,
    VelocityOverflow,
    ExertionOverflow,
    LogisticsRevisionExhausted,
}

impl Display for PlayerBodyMotionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CoordinateOverflow => {
                formatter.write_str("player body motion exceeded continuous world coordinates")
            }
            Self::VelocityOverflow => {
                formatter.write_str("player body velocity exceeded continuous motion range")
            }
            Self::ExertionOverflow => {
                formatter.write_str("control-rate locomotion exertion exceeded supported range")
            }
            Self::LogisticsRevisionExhausted => {
                formatter.write_str("logistics revision space is exhausted by player body motion")
            }
        }
    }
}

impl Error for PlayerBodyMotionError {}

pub use cover::{PlayerOverheadCoverAssessment, assess_player_overhead_cover};
pub use walk_projection::{
    PlayerWalkProjection, PlayerWalkProjectionError, project_player_adjacent_walk,
    project_player_walk_path, project_player_walk_path_for_load,
};

/// Observable continuous-body consequence of one control tick.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerBodyMotionOutcome {
    before: PlayerBodyState,
    after: PlayerBodyState,
    grounded: bool,
    fall_impact: Option<PlayerFallImpactOutcome>,
}

impl PlayerBodyMotionOutcome {
    #[must_use]
    pub const fn before(self) -> PlayerBodyState {
        self.before
    }

    #[must_use]
    pub const fn after(self) -> PlayerBodyState {
        self.after
    }

    #[must_use]
    pub const fn grounded(self) -> bool {
        self.grounded
    }

    #[must_use]
    pub const fn fall_impact(self) -> Option<PlayerFallImpactOutcome> {
        self.fall_impact
    }
}

/// Observable downward landing consequence captured by the continuous body solver.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerFallImpactOutcome {
    impact_speed: Speed,
    vitality_loss_ppm: u32,
}

impl PlayerFallImpactOutcome {
    #[must_use]
    pub const fn impact_speed(self) -> Speed {
        self.impact_speed
    }

    #[must_use]
    pub const fn vitality_loss_ppm(self) -> u32 {
        self.vitality_loss_ppm
    }
}

pub(crate) struct PlayerBodyMotionPlan {
    expected_logistics_revision: u64,
    next_logistics_revision: u64,
    expected_body: PlayerBodyState,
    body: PlayerBodyState,
    locomotion_exertion: LocomotionExertionAccumulator,
    settled_locomotion_exertion: SurvivalExertion,
    pending_fall_vitality_loss_ppm: u32,
    settled_fall_vitality_loss_ppm: u32,
    outcome: Option<PlayerBodyMotionOutcome>,
}

impl PlayerBodyMotionPlan {
    pub(crate) const fn settled_locomotion_exertion(&self) -> SurvivalExertion {
        self.settled_locomotion_exertion
    }

    pub(crate) const fn settled_fall_vitality_loss_ppm(&self) -> u32 {
        self.settled_fall_vitality_loss_ppm
    }
}

#[derive(Clone, Copy)]
struct PlayerControlAvailability {
    alive: bool,
    busy: bool,
    grounded: bool,
}

impl PlayerControlAvailability {
    const fn accepts_ground_control(self) -> bool {
        self.alive && self.grounded && !self.busy
    }
}

pub(crate) fn decide_player_body_motion(
    registries: &Registries,
    state: &AppState,
    input: PlayerControlInput,
    settle_pending_exertion: bool,
) -> Result<Option<PlayerBodyMotionPlan>, PlayerBodyMotionError> {
    let Some(player) = state.logistics().player().copied() else {
        return Ok(None);
    };
    let locomotion = registries.survival().physiology().locomotion();
    let Some(bounds) = horizontal_voxel_bounds(player.body(), locomotion) else {
        return Err(PlayerBodyMotionError::CoordinateOverflow);
    };
    if !horizontal_chunks_generated(state.world(), bounds) {
        return Ok(None);
    }

    let before = player.body();
    let availability = PlayerControlAvailability {
        alive: state
            .survival()
            .player()
            .is_some_and(|survival| survival.vitality() != Vitality::ZERO),
        busy: state.player_work().active().is_some(),
        grounded: supporting_floor_y(registries, state, before, bounds)
            == Some(before.foot_position().y_nanometers()),
    };
    let horizontal_body = resolve_horizontal_control(
        registries,
        state,
        before,
        locomotion,
        availability,
        input,
        None,
    )?;
    let (after, grounded, fall_impact) = resolve_vertical_control(
        registries,
        state,
        horizontal_body,
        locomotion,
        availability,
        input,
    )?;
    let exertion_before = player.locomotion_exertion();
    let pending_fall_before = player.pending_fall_vitality_loss_ppm();
    let (locomotion_exertion, settled_locomotion_exertion) = resolve_control_exertion(
        state,
        before,
        after,
        locomotion,
        availability,
        input,
        settle_pending_exertion,
    )?;
    let settled_fall_vitality_loss_ppm = if settle_pending_exertion {
        pending_fall_before
    } else {
        0
    };
    let pending_fall_base = if settle_pending_exertion {
        0
    } else {
        pending_fall_before
    };
    let pending_fall_vitality_loss_ppm = pending_fall_base
        .saturating_add(fall_impact.map_or(0, PlayerFallImpactOutcome::vitality_loss_ppm))
        .min(crate::core::arithmetic::NORMALIZED_PARTS_PER_MILLION);
    if after == before
        && locomotion_exertion == exertion_before
        && pending_fall_vitality_loss_ppm == pending_fall_before
    {
        return Ok(None);
    }
    if !state.can_spend_logistics_revisions(1) {
        return Err(PlayerBodyMotionError::LogisticsRevisionExhausted);
    }
    let expected_logistics_revision = state.logistics().revision();
    let next_logistics_revision = expected_logistics_revision
        .checked_add(1)
        .unwrap_or_else(|| unreachable!("logistics headroom check includes player body motion"));
    Ok(Some(PlayerBodyMotionPlan {
        expected_logistics_revision,
        next_logistics_revision,
        expected_body: before,
        body: after,
        locomotion_exertion,
        settled_locomotion_exertion,
        pending_fall_vitality_loss_ppm,
        settled_fall_vitality_loss_ppm,
        outcome: (after != before).then_some(PlayerBodyMotionOutcome {
            before,
            after,
            grounded,
            fall_impact,
        }),
    }))
}

fn resolve_horizontal_control(
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
    let target_speed = match stance {
        PlayerStance::Crouching => locomotion.crouching_speed(),
        PlayerStance::Standing => {
            let (carried, capacity) = projected_load.unwrap_or_else(|| {
                let carrying = assess_player_carrying(state).unwrap_or_else(|| {
                    unreachable!("generated body motion requires admitted carrying")
                });
                (carrying.stored(), carrying.capacity())
            });
            if input.sprint() {
                locomotion.sprinting_speed_for_load(carried, capacity)
            } else {
                locomotion.walking_speed_for_load(carried, capacity)
            }
        }
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

pub(crate) fn apply_player_body_motion(
    state: &mut AppState,
    plan: Option<PlayerBodyMotionPlan>,
) -> Option<PlayerBodyMotionOutcome> {
    let plan = plan?;
    state.logistics_state_mut().apply_player_body_motion(
        plan.expected_logistics_revision,
        plan.next_logistics_revision,
        plan.expected_body,
        plan.body,
        plan.locomotion_exertion,
        plan.pending_fall_vitality_loss_ppm,
    );
    plan.outcome
}

fn integrate_velocity(velocity_nanometers_per_second: i64) -> Result<i64, PlayerBodyMotionError> {
    let displacement = i128::from(velocity_nanometers_per_second)
        .checked_mul(i128::from(CONTROL_TICK_DURATION.microseconds()))
        .ok_or(PlayerBodyMotionError::CoordinateOverflow)?
        / MICROSECONDS_PER_SECOND;
    i64::try_from(displacement).map_err(|_| PlayerBodyMotionError::CoordinateOverflow)
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
#[path = "kinematics/acceleration_tests.rs"]
mod acceleration_tests;
