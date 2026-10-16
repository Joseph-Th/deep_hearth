//! Deterministic control-rate player-body integration against authoritative block cells.

mod horizontal;

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::{CONTROL_TICK_DURATION, CONTROL_TICKS_PER_SIMULATION_TICK, TickSpan};
use crate::registry::Registries;
use crate::spatial::{
    HorizontalVelocity, NANOMETERS_PER_VOXEL, VerticalVelocity, VoxelCoord, WorldPoint,
};
use crate::survival::{
    LocomotionDefinition, SurvivalExertion, SurvivalResourceBudget,
    SurvivalResourceProjectionError, Vitality, project_survival_resource_budget,
};
use crate::world::{BlockCollisionShape, ChunkCoord, WorldChunk, WorldRegistry, WorldState};

use super::state::LocomotionExertionAccumulator;
use super::{
    CONTROL_INPUT_PARTS_PER_MILLION, HorizontalControlInput, PlayerBodyState, PlayerControlInput,
    PlayerLogisticsRecord, PlayerStance, assess_player_carrying,
};
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

/// Failure to project one explicit, caller-supplied horizontal walk path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerWalkProjectionError {
    PathEmpty,
    ProjectedCapacityZero,
    ProjectedLoadExceedsCapacity {
        carried: Mass,
        capacity: Mass,
    },
    PlayerNotAdmitted,
    RequiresSimulationBoundary,
    PlayerBusy,
    PlayerNotStanding,
    PlayerAlreadyMoving,
    PlayerNotGrounded,
    PendingLocomotionExertion,
    DestinationNotHorizontalAdjacent {
        origin: VoxelCoord,
        destination: VoxelCoord,
    },
    DestinationUnreachableBeforeBoundary {
        origin: VoxelCoord,
        destination: VoxelCoord,
    },
    DestinationOvershot {
        destination: VoxelCoord,
        actual: VoxelCoord,
    },
    BodyMotion(PlayerBodyMotionError),
    ResourceProjection(SurvivalResourceProjectionError),
}

impl Display for PlayerWalkProjectionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PathEmpty => formatter.write_str("walk projection path is empty"),
            Self::ProjectedCapacityZero => {
                formatter.write_str("walk projection carrying capacity is zero")
            }
            Self::ProjectedLoadExceedsCapacity { carried, capacity } => write!(
                formatter,
                "walk projection carried mass {} mg exceeds capacity {} mg",
                carried.milligrams(),
                capacity.milligrams()
            ),
            Self::PlayerNotAdmitted => formatter.write_str("player is not admitted"),
            Self::RequiresSimulationBoundary => {
                formatter.write_str("walk projection requires a settled simulation boundary")
            }
            Self::PlayerBusy => formatter.write_str("player is already performing work"),
            Self::PlayerNotStanding => {
                formatter.write_str("player must be standing to project a walk")
            }
            Self::PlayerAlreadyMoving => {
                formatter.write_str("player must be motionless to project a walk")
            }
            Self::PlayerNotGrounded => {
                formatter.write_str("player must be grounded to project a walk")
            }
            Self::PendingLocomotionExertion => formatter
                .write_str("player must settle prior locomotion exertion before projecting a walk"),
            Self::DestinationNotHorizontalAdjacent {
                origin,
                destination,
            } => write!(
                formatter,
                "walk destination {destination:?} is not horizontally face-adjacent to {origin:?}"
            ),
            Self::DestinationUnreachableBeforeBoundary {
                origin,
                destination,
            } => write!(
                formatter,
                "walk from {origin:?} to {destination:?} cannot settle before the next simulation boundary"
            ),
            Self::DestinationOvershot {
                destination,
                actual,
            } => write!(
                formatter,
                "walk toward {destination:?} entered unexpected voxel {actual:?}"
            ),
            Self::BodyMotion(error) => write!(formatter, "player body motion failed: {error}"),
            Self::ResourceProjection(error) => {
                write!(formatter, "walk survival projection failed: {error:?}")
            }
        }
    }
}

impl Error for PlayerWalkProjectionError {}

/// Exact control-time and next-boundary survival cost for an explicit local walk path.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerWalkProjection {
    origin: VoxelCoord,
    destination: VoxelCoord,
    control_ticks: u64,
    locomotion_exertion: SurvivalExertion,
    resource_budget: SurvivalResourceBudget,
}

impl PlayerWalkProjection {
    #[must_use]
    pub const fn origin(self) -> VoxelCoord {
        self.origin
    }

    #[must_use]
    pub const fn destination(self) -> VoxelCoord {
        self.destination
    }

    #[must_use]
    pub const fn control_ticks(self) -> u64 {
        self.control_ticks
    }

    #[must_use]
    pub const fn locomotion_exertion(self) -> SurvivalExertion {
        self.locomotion_exertion
    }

    #[must_use]
    pub const fn resource_budget(self) -> SurvivalResourceBudget {
        self.resource_budget
    }
}

/// Projects one horizontal face-adjacent walk from a settled slow boundary.
///
/// This is a convenience wrapper over [`project_player_walk_path`]. It does not generate routes.
pub fn project_player_adjacent_walk(
    registries: &Registries,
    state: &AppState,
    destination: VoxelCoord,
) -> Result<PlayerWalkProjection, PlayerWalkProjectionError> {
    project_player_walk_path(registries, state, &[destination])
}

/// Projects a caller-supplied sequence of horizontal face-adjacent walk destinations.
///
/// The current player must be standing, grounded, motionless, idle, and at a slow-simulation
/// boundary with prior locomotion exertion settled. Every leg is resolved through the canonical
/// body/collision kernel and the whole path must settle before the next slow boundary. The result's
/// resource budget is the exact basal-plus-locomotion charge that will settle at that boundary.
/// This function deliberately does not discover, rank, or route between destinations.
pub fn project_player_walk_path(
    registries: &Registries,
    state: &AppState,
    destinations: &[VoxelCoord],
) -> Result<PlayerWalkProjection, PlayerWalkProjectionError> {
    let carrying =
        assess_player_carrying(state).ok_or(PlayerWalkProjectionError::PlayerNotAdmitted)?;
    project_player_walk_path_for_load(
        registries,
        state,
        destinations,
        carrying.stored(),
        carrying.capacity(),
    )
}

/// Projects an explicit walk path using a hypothetical future carried load.
///
/// Ordinary walking speed is load-independent; only locomotion exertion is load-sensitive. This
/// lets callers price a planned return haul without manufacturing future inventory custody. The
/// requested load must fit the supplied body-carrying capacity. Route discovery remains caller-owned.
pub fn project_player_walk_path_for_load(
    registries: &Registries,
    state: &AppState,
    destinations: &[VoxelCoord],
    carried: Mass,
    capacity: Mass,
) -> Result<PlayerWalkProjection, PlayerWalkProjectionError> {
    if capacity.is_zero() {
        return Err(PlayerWalkProjectionError::ProjectedCapacityZero);
    }
    if carried > capacity {
        return Err(PlayerWalkProjectionError::ProjectedLoadExceedsCapacity { carried, capacity });
    }
    let destination = *destinations
        .last()
        .ok_or(PlayerWalkProjectionError::PathEmpty)?;
    let player = validate_walk_projection_start(registries, state)?;
    let origin = player.position();
    let mut projected = state.clone();
    let mut projected_exertion = player.locomotion_exertion();
    let mut control_ticks = 0_u64;
    let maximum_control_ticks = CONTROL_TICKS_PER_SIMULATION_TICK - 1;

    for &destination in destinations {
        project_walk_leg(
            registries,
            &mut projected,
            destination,
            &mut control_ticks,
            maximum_control_ticks,
            carried,
            capacity,
            &mut projected_exertion,
        )?;
    }

    let locomotion_exertion = projected_exertion.pending();
    let resource_budget = project_survival_resource_budget(
        registries.survival().physiology(),
        locomotion_exertion,
        TickSpan::new(1),
    )
    .map_err(PlayerWalkProjectionError::ResourceProjection)?;
    Ok(PlayerWalkProjection {
        origin,
        destination,
        control_ticks,
        locomotion_exertion,
        resource_budget,
    })
}

fn validate_walk_projection_start(
    registries: &Registries,
    state: &AppState,
) -> Result<PlayerLogisticsRecord, PlayerWalkProjectionError> {
    let player = state
        .logistics()
        .player()
        .copied()
        .ok_or(PlayerWalkProjectionError::PlayerNotAdmitted)?;
    if !state.control_tick().is_simulation_boundary() {
        return Err(PlayerWalkProjectionError::RequiresSimulationBoundary);
    }
    if state.player_work().active().is_some() {
        return Err(PlayerWalkProjectionError::PlayerBusy);
    }
    let body = player.body();
    if body.stance() != PlayerStance::Standing {
        return Err(PlayerWalkProjectionError::PlayerNotStanding);
    }
    let horizontal = body.horizontal_velocity();
    if horizontal.x_nanometers_per_second() != 0
        || horizontal.z_nanometers_per_second() != 0
        || body.vertical_velocity().nanometers_per_second() != 0
    {
        return Err(PlayerWalkProjectionError::PlayerAlreadyMoving);
    }
    if player.locomotion_exertion().pending() != SurvivalExertion::REST {
        return Err(PlayerWalkProjectionError::PendingLocomotionExertion);
    }
    if !player_body_is_grounded(registries, state) {
        return Err(PlayerWalkProjectionError::PlayerNotGrounded);
    }
    Ok(player)
}

fn project_walk_leg(
    registries: &Registries,
    projected: &mut AppState,
    destination: VoxelCoord,
    control_ticks: &mut u64,
    maximum_control_ticks: u64,
    carried: Mass,
    capacity: Mass,
    projected_exertion: &mut LocomotionExertionAccumulator,
) -> Result<(), PlayerWalkProjectionError> {
    let leg_origin = projected
        .logistics()
        .player()
        .map(|record| record.position())
        .unwrap_or_else(|| unreachable!("walk projection cannot remove the player"));
    if !leg_origin.is_face_adjacent_to(destination) || leg_origin.y() != destination.y() {
        return Err(
            PlayerWalkProjectionError::DestinationNotHorizontalAdjacent {
                origin: leg_origin,
                destination,
            },
        );
    }
    let movement = adjacent_walk_input(leg_origin, destination);
    while projected
        .logistics()
        .player()
        .is_some_and(|record| record.position() != destination)
    {
        ensure_walk_tick_available(
            *control_ticks,
            maximum_control_ticks,
            leg_origin,
            destination,
        )?;
        let before = projected
            .logistics()
            .player()
            .copied()
            .unwrap_or_else(|| unreachable!("walk projection cannot remove the player"))
            .body();
        let plan = decide_player_body_motion(registries, projected, movement, false)
            .map_err(PlayerWalkProjectionError::BodyMotion)?;
        if plan.is_none() {
            return Err(
                PlayerWalkProjectionError::DestinationUnreachableBeforeBoundary {
                    origin: leg_origin,
                    destination,
                },
            );
        }
        let _ = apply_player_body_motion(projected, plan);
        let after = projected
            .logistics()
            .player()
            .copied()
            .unwrap_or_else(|| unreachable!("walk projection cannot remove the player"))
            .body();
        accrue_projected_walk_exertion(
            registries,
            before,
            after,
            carried,
            capacity,
            projected_exertion,
        )?;
        *control_ticks += 1;
        let actual = projected
            .logistics()
            .player()
            .map(|record| record.position())
            .unwrap_or_else(|| unreachable!("walk projection cannot remove the player"));
        if actual != leg_origin && actual != destination {
            return Err(PlayerWalkProjectionError::DestinationOvershot {
                destination,
                actual,
            });
        }
    }
    settle_projected_walk_velocity(
        registries,
        projected,
        leg_origin,
        destination,
        control_ticks,
        maximum_control_ticks,
    )
}

fn accrue_projected_walk_exertion(
    registries: &Registries,
    before: PlayerBodyState,
    after: PlayerBodyState,
    carried: Mass,
    capacity: Mass,
    accumulator: &mut LocomotionExertionAccumulator,
) -> Result<(), PlayerWalkProjectionError> {
    if !horizontal_position_changed(before.foot_position(), after.foot_position()) {
        return Ok(());
    }
    let realized_velocity =
        realized_horizontal_velocity(before.foot_position(), after.foot_position())
            .map_err(PlayerWalkProjectionError::BodyMotion)?;
    let speed =
        horizontal_speed(realized_velocity).map_err(PlayerWalkProjectionError::BodyMotion)?;
    let exertion = registries
        .survival()
        .physiology()
        .locomotion()
        .exertion_for_horizontal_speed(carried, capacity, speed)
        .ok_or(PlayerWalkProjectionError::BodyMotion(
            PlayerBodyMotionError::ExertionOverflow,
        ))?;
    *accumulator =
        accumulator
            .checked_accrue(exertion)
            .ok_or(PlayerWalkProjectionError::BodyMotion(
                PlayerBodyMotionError::ExertionOverflow,
            ))?;
    Ok(())
}

fn settle_projected_walk_velocity(
    registries: &Registries,
    projected: &mut AppState,
    leg_origin: VoxelCoord,
    destination: VoxelCoord,
    control_ticks: &mut u64,
    maximum_control_ticks: u64,
) -> Result<(), PlayerWalkProjectionError> {
    loop {
        let body = projected
            .logistics()
            .player()
            .copied()
            .unwrap_or_else(|| unreachable!("walk projection cannot remove the player"))
            .body();
        let horizontal = body.horizontal_velocity();
        if horizontal.x_nanometers_per_second() == 0 && horizontal.z_nanometers_per_second() == 0 {
            return Ok(());
        }
        ensure_walk_tick_available(
            *control_ticks,
            maximum_control_ticks,
            leg_origin,
            destination,
        )?;
        let plan =
            decide_player_body_motion(registries, projected, PlayerControlInput::NEUTRAL, false)
                .map_err(PlayerWalkProjectionError::BodyMotion)?;
        let _ = apply_player_body_motion(projected, plan);
        *control_ticks += 1;
        let actual = projected
            .logistics()
            .player()
            .map(|record| record.position())
            .unwrap_or_else(|| unreachable!("walk projection cannot remove the player"));
        if actual != destination {
            return Err(PlayerWalkProjectionError::DestinationOvershot {
                destination,
                actual,
            });
        }
    }
}

fn ensure_walk_tick_available(
    control_ticks: u64,
    maximum_control_ticks: u64,
    origin: VoxelCoord,
    destination: VoxelCoord,
) -> Result<(), PlayerWalkProjectionError> {
    if control_ticks < maximum_control_ticks {
        Ok(())
    } else {
        Err(
            PlayerWalkProjectionError::DestinationUnreachableBeforeBoundary {
                origin,
                destination,
            },
        )
    }
}

fn adjacent_walk_input(origin: VoxelCoord, destination: VoxelCoord) -> PlayerControlInput {
    let x = i32::try_from(destination.x() - origin.x())
        .unwrap_or_else(|_| unreachable!("horizontal adjacent x delta fits i32"))
        * CONTROL_INPUT_PARTS_PER_MILLION;
    let z = i32::try_from(destination.z() - origin.z())
        .unwrap_or_else(|_| unreachable!("horizontal adjacent z delta fits i32"))
        * CONTROL_INPUT_PARTS_PER_MILLION;
    let horizontal = HorizontalControlInput::new(x, z)
        .unwrap_or_else(|_| unreachable!("horizontal adjacent walk input is normalized"));
    PlayerControlInput::new(horizontal)
}

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
    expected_body: PlayerBodyState,
    body: PlayerBodyState,
    locomotion_exertion: LocomotionExertionAccumulator,
    settled_locomotion_exertion: SurvivalExertion,
    outcome: Option<PlayerBodyMotionOutcome>,
}

impl PlayerBodyMotionPlan {
    pub(crate) const fn settled_locomotion_exertion(&self) -> SurvivalExertion {
        self.settled_locomotion_exertion
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

fn first_player_body_collision_with_lookup(
    registry: &WorldRegistry,
    body: PlayerBodyState,
    locomotion: LocomotionDefinition,
    mut lookup: impl FnMut(VoxelCoord) -> Option<crate::world::BlockStateId>,
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
                if lookup(voxel).is_some_and(|state| block_is_solid(registry, state)) {
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
        grounded: supporting_floor_y(registries.world(), state.world(), before, bounds)
            == Some(before.foot_position().y_nanometers()),
    };
    let horizontal_body =
        resolve_horizontal_control(registries, state, player, locomotion, availability, input)?;
    let (after, grounded) = resolve_vertical_control(
        registries,
        state,
        horizontal_body,
        locomotion,
        availability,
        input,
    )?;
    let exertion_before = player.locomotion_exertion();
    let (locomotion_exertion, settled_locomotion_exertion) = resolve_control_exertion(
        state,
        before,
        after,
        locomotion,
        availability,
        input,
        settle_pending_exertion,
    )?;
    if after == before && locomotion_exertion == exertion_before {
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
        outcome: (after != before).then_some(PlayerBodyMotionOutcome {
            before,
            after,
            grounded,
        }),
    }))
}

fn resolve_horizontal_control(
    registries: &Registries,
    state: &AppState,
    player: PlayerLogisticsRecord,
    locomotion: LocomotionDefinition,
    availability: PlayerControlAvailability,
    input: PlayerControlInput,
) -> Result<PlayerBodyState, PlayerBodyMotionError> {
    let before = player.body();
    let desired_stance = if availability.alive && !availability.busy && input.crouch() {
        PlayerStance::Crouching
    } else if availability.alive && !availability.busy {
        PlayerStance::Standing
    } else {
        before.stance()
    };
    let stance = resolve_stance(
        registries.world(),
        state.world(),
        before,
        locomotion,
        desired_stance,
    );
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
    let (mut position, collided_x, collided_z) = resolve_horizontal_motion(
        registries.world(),
        state.world(),
        stance_body,
        locomotion,
        velocity,
    )?;
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
    let support = supporting_floor_y(
        registries.world(),
        state.world(),
        candidate,
        candidate_bounds,
    );
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
) -> Result<(PlayerBodyState, bool), PlayerBodyMotionError> {
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
        registries.world(),
        state.world(),
        horizontal_body,
        locomotion,
        horizontal_bounds,
        proposed_y,
        vertical_velocity,
    )?;
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

fn resolve_control_exertion(
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
    if !availability.accepts_ground_control()
        || input.horizontal().is_neutral()
        || !horizontal_position_changed(before.foot_position(), after.foot_position())
    {
        return Ok((accumulator, settled));
    }
    let carrying = assess_player_carrying(state)
        .unwrap_or_else(|| unreachable!("generated body motion requires admitted carrying"));
    let realized_velocity =
        realized_horizontal_velocity(before.foot_position(), after.foot_position())?;
    let speed = horizontal_speed(realized_velocity)?;
    let exertion = locomotion
        .exertion_for_horizontal_speed(carrying.stored(), carrying.capacity(), speed)
        .ok_or(PlayerBodyMotionError::ExertionOverflow)?;
    accumulator = accumulator
        .checked_accrue(exertion)
        .ok_or(PlayerBodyMotionError::ExertionOverflow)?;
    Ok((accumulator, settled))
}

fn horizontal_position_changed(before: WorldPoint, after: WorldPoint) -> bool {
    before.x_nanometers() != after.x_nanometers() || before.z_nanometers() != after.z_nanometers()
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
    registry: &WorldRegistry,
    world: &WorldState,
    body: PlayerBodyState,
    locomotion: LocomotionDefinition,
    desired: PlayerStance,
) -> PlayerStance {
    if desired == body.stance() || desired == PlayerStance::Crouching {
        return desired;
    }
    let standing = body.with_stance(PlayerStance::Standing);
    if first_player_body_collision(registry, world, standing, locomotion).is_none() {
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
    let magnitude = i128::try_from(integer_sqrt(
        u128::try_from(magnitude_squared).map_err(|_| PlayerBodyMotionError::VelocityOverflow)?,
    ))
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

fn integer_sqrt(value: u128) -> u128 {
    if value < 2 {
        return value;
    }
    let mut estimate = 1_u128 << ((128 - value.leading_zeros() as usize).div_ceil(2));
    loop {
        let next = (estimate + value / estimate) / 2;
        if next >= estimate {
            return estimate;
        }
        estimate = next;
    }
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

    let height = length_nanometers(body_height(locomotion, body.stance()))
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
