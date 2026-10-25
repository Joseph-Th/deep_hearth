//! Deterministic control-rate player-body integration against authoritative block cells.

mod cover;
mod exertion;
mod horizontal;
mod walk_projection;

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Speed;
use crate::core::state::AppState;
use crate::core::time::CONTROL_TICK_DURATION;
use crate::registry::Registries;
use crate::spatial::{
    HorizontalVelocity, NANOMETERS_PER_VOXEL, VerticalVelocity, VoxelBounds, VoxelCoord, WorldPoint,
};
use crate::survival::{LocomotionDefinition, SurvivalExertion, Vitality};
use crate::world::{
    BlockCollisionShape, ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y, WorldChunk, WorldRegistry,
    WorldState,
};

use super::state::LocomotionExertionAccumulator;
use super::{
    CONTROL_INPUT_PARTS_PER_MILLION, HorizontalControlInput, PlayerBodyState, PlayerControlInput,
    PlayerLogisticsRecord, PlayerStance, assess_player_carrying,
};
use exertion::resolve_control_exertion;
use horizontal::{player_voxel_change_allowed, resolve_horizontal_motion};

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
    supporting_floor_y(registries, state, player.body(), bounds)
        == Some(player.body().foot_position().y_nanometers())
}

pub(crate) fn first_player_body_collision(
    registry: &WorldRegistry,
    world: &WorldState,
    body: PlayerBodyState,
    locomotion: LocomotionDefinition,
) -> Option<VoxelCoord> {
    first_player_body_collision_where(body, locomotion, |voxel| {
        // Ungenerated horizontal chunks are not yet collision authority, but a generated
        // column cannot admit a body into vertical cells outside the supported world.
        world.get_chunk(ChunkCoord::containing(voxel)).is_some()
            && world_cell_blocks_motion(registry, world, voxel)
    })
}

/// A creature does not cease to be solid when hidden from the player.
/// Admission and trusted-load checking share the continuous body footprint.
pub(crate) fn first_player_temple_caste_collision(
    registries: &Registries,
    state: &AppState,
    body: PlayerBodyState,
) -> Option<VoxelCoord> {
    let locomotion = registries.survival().physiology().locomotion();
    first_player_body_collision_where(body, locomotion, |voxel| {
        crate::kobolds::temple_caste_occupies_voxel(registries, state, voxel)
    })
}

/// Fully spanning built structural members are solid to a continuous player
/// body in generated terrain. Inert plans and narrow supports remain traversable.
pub(crate) fn first_player_structure_collision(
    registries: &Registries,
    state: &AppState,
    body: PlayerBodyState,
) -> Option<VoxelCoord> {
    if state.world().generation_profile().is_none() {
        return None;
    }
    let locomotion = registries.survival().physiology().locomotion();
    first_player_body_collision_where(body, locomotion, |voxel| {
        structure_blocks_player_cell(state, voxel)
    })
}

fn structure_blocks_player_cell(state: &AppState, voxel: VoxelCoord) -> bool {
    state.world().generation_profile().is_some() && state.structures().blocks_full_cell(voxel)
}

pub(crate) fn first_player_body_collision_in_chunk(
    registry: &WorldRegistry,
    chunk: &WorldChunk,
    body: PlayerBodyState,
    locomotion: LocomotionDefinition,
) -> Option<VoxelCoord> {
    first_player_body_collision_where(body, locomotion, |voxel| {
        ChunkCoord::containing(voxel) == chunk.coord()
            && (!(WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE).contains(&voxel.y())
                || block_is_solid(registry, chunk.block_at(voxel)))
    })
}

/// Physical intersection of a proposed full-cell member and the continuous player body.
/// Pre-generation authored traversal graphs have no voxel collision authority.
pub(crate) fn first_player_body_overlap_with_bounds(
    registries: &Registries,
    state: &AppState,
    bounds: VoxelBounds,
) -> Option<VoxelCoord> {
    if state.world().generation_profile().is_none() {
        return None;
    }
    let body = state.logistics().player()?.body();
    let locomotion = registries.survival().physiology().locomotion();
    first_player_body_collision_where(body, locomotion, |voxel| bounds.has_voxel(voxel))
}

pub(crate) fn player_body_occupies_voxel(
    registries: &Registries,
    state: &AppState,
    voxel: VoxelCoord,
) -> bool {
    let Some(player) = state.logistics().player().copied() else {
        return false;
    };
    let locomotion = registries.survival().physiology().locomotion();
    let Some((min_x, max_x, min_z, max_z)) = horizontal_voxel_bounds(player.body(), locomotion)
    else {
        return true;
    };
    let foot_y = player.body().foot_position().y_nanometers();
    let Some(height) = length_nanometers(body_height(locomotion, player.body().stance())) else {
        return true;
    };
    let Some(top_exclusive) = foot_y.checked_add(height) else {
        return true;
    };
    let Some(top_inclusive) = top_exclusive.checked_sub(1) else {
        return true;
    };
    let min_y = foot_y.div_euclid(NANOMETERS_PER_VOXEL);
    let max_y = top_inclusive.div_euclid(NANOMETERS_PER_VOXEL);
    voxel.x() >= min_x
        && voxel.x() <= max_x
        && voxel.y() >= min_y
        && voxel.y() <= max_y
        && voxel.z() >= min_z
        && voxel.z() <= max_z
}

fn first_player_body_collision_where(
    body: PlayerBodyState,
    locomotion: LocomotionDefinition,
    mut collides: impl FnMut(VoxelCoord) -> bool,
) -> Option<VoxelCoord> {
    let (min_x, max_x, min_z, max_z) = horizontal_voxel_bounds(body, locomotion)?;
    let foot_y = body.foot_position().y_nanometers();
    let height = length_nanometers(body_height(locomotion, body.stance()))?;
    let top_exclusive = foot_y.checked_add(height)?;
    let min_y = foot_y.div_euclid(NANOMETERS_PER_VOXEL);
    let max_y = top_exclusive
        .checked_sub(1)?
        .div_euclid(NANOMETERS_PER_VOXEL);
    for y in min_y..=max_y {
        for z in min_z..=max_z {
            for x in min_x..=max_x {
                let voxel = VoxelCoord::new(x, y, z);
                if collides(voxel) {
                    return Some(voxel);
                }
            }
        }
    }
    None
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
        player,
        before,
        locomotion,
        availability,
        input,
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
    player: PlayerLogisticsRecord,
    before: PlayerBodyState,
    locomotion: LocomotionDefinition,
    availability: PlayerControlAvailability,
    input: PlayerControlInput,
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
    let target_speed = match stance {
        PlayerStance::Crouching => locomotion.crouching_speed(),
        PlayerStance::Standing if input.sprint() => {
            let carrying = assess_player_carrying(state).unwrap_or_else(|| {
                unreachable!("generated body motion requires admitted carrying")
            });
            locomotion.sprinting_speed_for_load(carrying.stored(), carrying.capacity())
        }
        PlayerStance::Standing => locomotion.walking_speed(),
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

fn resolve_vertical_control(
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

pub(super) const fn body_height(
    locomotion: LocomotionDefinition,
    stance: PlayerStance,
) -> crate::core::quantity::Length {
    match stance {
        PlayerStance::Standing => locomotion.standing_height(),
        PlayerStance::Crouching => locomotion.crouching_height(),
    }
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

fn supporting_floor_y(
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

/// All movement axes, standing clearance and grounding share solid-cell authority.
/// Concealed temple residents remain physical obstacles just like generated rock.
fn player_cell_blocks_motion(registries: &Registries, state: &AppState, voxel: VoxelCoord) -> bool {
    world_cell_blocks_motion(registries.world(), state.world(), voxel)
        || crate::kobolds::temple_caste_occupies_voxel(registries, state, voxel)
        || structure_blocks_player_cell(state, voxel)
}

fn block_is_solid(registry: &WorldRegistry, state: crate::world::BlockStateId) -> bool {
    registry
        .block_for_state(state)
        .is_some_and(|block| block.collision() == BlockCollisionShape::FullCube)
}

/// Collision authority is the same on every movement axis: unknown cells are never air.
/// The separate player-admission query checks whether a chunk is generated before using this.
pub(super) fn world_cell_blocks_motion(
    registry: &WorldRegistry,
    world: &WorldState,
    voxel: VoxelCoord,
) -> bool {
    world
        .block_at(voxel)
        .is_none_or(|state| block_is_solid(registry, state))
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

#[cfg(test)]
#[path = "kinematics/acceleration_tests.rs"]
mod acceleration_tests;
