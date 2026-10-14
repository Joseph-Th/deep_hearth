//! Deterministic control-rate player-body integration against authoritative block cells.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::state::AppState;
use crate::core::time::CONTROL_TICK_DURATION;
use crate::registry::Registries;
use crate::spatial::{NANOMETERS_PER_VOXEL, VerticalVelocity, VoxelCoord, WorldPoint};
use crate::survival::LocomotionDefinition;
use crate::world::{BlockCollisionShape, ChunkCoord, WorldChunk, WorldRegistry, WorldState};

use super::PlayerBodyState;

const MICROSECONDS_PER_SECOND: i128 = 1_000_000;
const NANOMETERS_PER_MICROMETER: i128 = 1_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerBodyMotionError {
    CoordinateOverflow,
    VelocityOverflow,
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
            Self::LogisticsRevisionExhausted => {
                formatter.write_str("logistics revision space is exhausted by player body motion")
            }
        }
    }
}

impl Error for PlayerBodyMotionError {}

/// Observable continuous-body consequence of one control tick.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerBodyMotionOutcome {
    before: PlayerBodyState,
    after: PlayerBodyState,
    grounded: bool,
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
}

pub(crate) struct PlayerBodyMotionPlan {
    expected_logistics_revision: u64,
    next_logistics_revision: u64,
    outcome: PlayerBodyMotionOutcome,
}

pub(crate) fn player_body_uses_generated_world(
    state: &AppState,
    locomotion: LocomotionDefinition,
) -> bool {
    state.logistics().player().is_some_and(|player| {
        horizontal_voxel_bounds(player.body(), locomotion)
            .is_some_and(|bounds| horizontal_chunks_generated(state.world(), bounds))
    })
}

pub(crate) fn player_body_fits_continuous_world(
    body: PlayerBodyState,
    locomotion: LocomotionDefinition,
) -> bool {
    horizontal_voxel_bounds(body, locomotion).is_some()
        && length_nanometers(locomotion.standing_height()).is_some_and(|height| {
            body.foot_position()
                .y_nanometers()
                .checked_add(height)
                .is_some()
        })
}

pub(crate) fn player_body_is_grounded(registries: &Registries, state: &AppState) -> bool {
    let Some(player) = state.logistics().player().copied() else {
        return false;
    };
    let locomotion = registries.survival().physiology().locomotion();
    let Some(bounds) = horizontal_voxel_bounds(player.body(), locomotion) else {
        return false;
    };
    if !horizontal_chunks_generated(state.world(), bounds) {
        return false;
    }
    supporting_floor_y(registries.world(), state.world(), player.body(), bounds)
        == Some(player.body().foot_position().y_nanometers())
}

pub(crate) fn first_player_body_collision(
    registry: &WorldRegistry,
    world: &WorldState,
    body: PlayerBodyState,
    locomotion: LocomotionDefinition,
) -> Option<VoxelCoord> {
    first_player_body_collision_with_lookup(registry, body, locomotion, |voxel| {
        world.block_at(voxel)
    })
}

pub(crate) fn first_player_body_collision_in_chunk(
    registry: &WorldRegistry,
    chunk: &WorldChunk,
    body: PlayerBodyState,
    locomotion: LocomotionDefinition,
) -> Option<VoxelCoord> {
    first_player_body_collision_with_lookup(registry, body, locomotion, |voxel| {
        (ChunkCoord::containing(voxel) == chunk.coord()).then(|| chunk.block_at(voxel))
    })
}

fn first_player_body_collision_with_lookup(
    registry: &WorldRegistry,
    body: PlayerBodyState,
    locomotion: LocomotionDefinition,
    mut lookup: impl FnMut(VoxelCoord) -> Option<crate::world::BlockStateId>,
) -> Option<VoxelCoord> {
    let (min_x, max_x, min_z, max_z) = horizontal_voxel_bounds(body, locomotion)?;
    let foot_y = body.foot_position().y_nanometers();
    let height = length_nanometers(locomotion.standing_height())?;
    let top_exclusive = foot_y.checked_add(height)?;
    let min_y = foot_y.div_euclid(NANOMETERS_PER_VOXEL);
    let max_y = top_exclusive
        .checked_sub(1)?
        .div_euclid(NANOMETERS_PER_VOXEL);
    for y in min_y..=max_y {
        for z in min_z..=max_z {
            for x in min_x..=max_x {
                let voxel = VoxelCoord::new(x, y, z);
                if lookup(voxel).is_some_and(|state| block_is_solid(registry, state)) {
                    return Some(voxel);
                }
            }
        }
    }
    None
}

pub(crate) fn decide_player_body_motion(
    registries: &Registries,
    state: &AppState,
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
    let velocity_before = before.vertical_velocity();
    let gravity_delta = gravity_velocity_delta(registries)?;
    let maximum_fall_speed = i64::try_from(locomotion.maximum_fall_speed().nanometers_per_second())
        .map_err(|_| PlayerBodyMotionError::VelocityOverflow)?;
    let vertical_velocity = velocity_before
        .nanometers_per_second()
        .checked_sub(gravity_delta)
        .ok_or(PlayerBodyMotionError::VelocityOverflow)?
        .max(-maximum_fall_speed);
    let vertical_displacement = integrate_velocity(vertical_velocity)?;
    let proposed_y = before
        .foot_position()
        .y_nanometers()
        .checked_add(vertical_displacement)
        .ok_or(PlayerBodyMotionError::CoordinateOverflow)?;
    let (resolved_y, resolved_vertical_velocity, grounded) = resolve_vertical_collision(
        registries.world(),
        state.world(),
        before,
        locomotion,
        bounds,
        proposed_y,
        vertical_velocity,
    )?;
    let position_before = before.foot_position();
    let position_after = WorldPoint::new(
        position_before.x_nanometers(),
        resolved_y,
        position_before.z_nanometers(),
    );
    let velocity_after = VerticalVelocity::new(resolved_vertical_velocity);
    let after = before.with_motion(position_after, velocity_after);
    if after == before {
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
        outcome: PlayerBodyMotionOutcome {
            before,
            after,
            grounded,
        },
    }))
}

pub(crate) fn apply_player_body_motion(
    state: &mut AppState,
    plan: Option<PlayerBodyMotionPlan>,
) -> Option<PlayerBodyMotionOutcome> {
    let plan = plan?;
    state.logistics_state_mut().apply_player_body_motion(
        plan.expected_logistics_revision,
        plan.next_logistics_revision,
        plan.outcome.before,
        plan.outcome.after,
    );
    Some(plan.outcome)
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

fn integrate_velocity(velocity_nanometers_per_second: i64) -> Result<i64, PlayerBodyMotionError> {
    let displacement = i128::from(velocity_nanometers_per_second)
        .checked_mul(i128::from(CONTROL_TICK_DURATION.microseconds()))
        .ok_or(PlayerBodyMotionError::CoordinateOverflow)?
        / MICROSECONDS_PER_SECOND;
    i64::try_from(displacement).map_err(|_| PlayerBodyMotionError::CoordinateOverflow)
}

fn resolve_vertical_collision(
    registry: &WorldRegistry,
    world: &WorldState,
    body: PlayerBodyState,
    locomotion: LocomotionDefinition,
    bounds: (i64, i64, i64, i64),
    proposed_y: i64,
    proposed_velocity: i64,
) -> Result<(i64, i64, bool), PlayerBodyMotionError> {
    let current_y = body.foot_position().y_nanometers();
    if proposed_y <= current_y {
        let floor = crossed_floor_y(registry, world, bounds, proposed_y, current_y)?;
        if let Some(floor_y) = floor {
            return Ok((floor_y, 0, true));
        }
        return Ok((proposed_y, proposed_velocity, false));
    }

    let height = length_nanometers(locomotion.standing_height())
        .ok_or(PlayerBodyMotionError::CoordinateOverflow)?;
    let current_top = current_y
        .checked_add(height)
        .ok_or(PlayerBodyMotionError::CoordinateOverflow)?;
    let proposed_top = proposed_y
        .checked_add(height)
        .ok_or(PlayerBodyMotionError::CoordinateOverflow)?;
    if let Some(ceiling) = crossed_ceiling_y(registry, world, bounds, current_top, proposed_top)? {
        let foot = ceiling
            .checked_sub(height)
            .ok_or(PlayerBodyMotionError::CoordinateOverflow)?;
        return Ok((foot, 0, false));
    }
    Ok((proposed_y, proposed_velocity, false))
}

fn crossed_floor_y(
    registry: &WorldRegistry,
    world: &WorldState,
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
                if world
                    .block_at(VoxelCoord::new(x, y, z))
                    .is_some_and(|state| block_is_solid(registry, state))
                {
                    floor = Some(floor.map_or(top, |existing: i64| existing.max(top)));
                }
            }
        }
    }
    Ok(floor)
}

fn crossed_ceiling_y(
    registry: &WorldRegistry,
    world: &WorldState,
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
                if world
                    .block_at(VoxelCoord::new(x, y, z))
                    .is_some_and(|state| block_is_solid(registry, state))
                {
                    ceiling = Some(ceiling.map_or(bottom, |existing: i64| existing.min(bottom)));
                }
            }
        }
    }
    Ok(ceiling)
}

fn supporting_floor_y(
    registry: &WorldRegistry,
    world: &WorldState,
    body: PlayerBodyState,
    bounds: (i64, i64, i64, i64),
) -> Option<i64> {
    let foot = body.foot_position().y_nanometers();
    let probe = foot.checked_sub(1)?;
    crossed_floor_y(registry, world, bounds, probe, foot)
        .ok()
        .flatten()
}

fn block_is_solid(registry: &WorldRegistry, state: crate::world::BlockStateId) -> bool {
    registry
        .block_for_state(state)
        .is_some_and(|block| block.collision() == BlockCollisionShape::FullCube)
}

fn horizontal_voxel_bounds(
    body: PlayerBodyState,
    locomotion: LocomotionDefinition,
) -> Option<(i64, i64, i64, i64)> {
    let width = length_nanometers(locomotion.body_width())?;
    let half = width / 2;
    let position = body.foot_position();
    let min_x_nm = position.x_nanometers().checked_sub(half)?;
    let max_x_nm = position.x_nanometers().checked_add(width - half)?;
    let min_z_nm = position.z_nanometers().checked_sub(half)?;
    let max_z_nm = position.z_nanometers().checked_add(width - half)?;
    let max_x_inclusive = max_x_nm.checked_sub(1)?;
    let max_z_inclusive = max_z_nm.checked_sub(1)?;
    Some((
        min_x_nm.div_euclid(NANOMETERS_PER_VOXEL),
        max_x_inclusive.div_euclid(NANOMETERS_PER_VOXEL),
        min_z_nm.div_euclid(NANOMETERS_PER_VOXEL),
        max_z_inclusive.div_euclid(NANOMETERS_PER_VOXEL),
    ))
}

fn horizontal_chunks_generated(world: &WorldState, bounds: (i64, i64, i64, i64)) -> bool {
    let (min_x, max_x, min_z, max_z) = bounds;
    for z in min_z..=max_z {
        for x in min_x..=max_x {
            if world
                .get_chunk(ChunkCoord::containing(VoxelCoord::new(x, 0, z)))
                .is_none()
            {
                return false;
            }
        }
    }
    true
}

fn length_nanometers(length: crate::core::quantity::Length) -> Option<i64> {
    i64::try_from(u128::from(length.micrometers()).checked_mul(NANOMETERS_PER_MICROMETER as u128)?)
        .ok()
}
