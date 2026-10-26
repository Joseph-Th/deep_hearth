//! Read-only local walk projections through the authoritative player-body kernel.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::{CONTROL_TICKS_PER_SIMULATION_TICK, TickSpan};
use crate::player::{PlayerActionValidationError, validate_player_action};
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::survival::{
    SurvivalExertion, SurvivalResourceBudget, SurvivalResourceProjectionError, Vitality,
    project_survival_resource_budget,
};

use super::exertion::walking_exertion;
use super::{
    CONTROL_INPUT_PARTS_PER_MILLION, HorizontalControlInput, LocomotionExertionAccumulator,
    PlayerBodyMotionError, PlayerBodyState, PlayerControlAvailability, PlayerControlInput,
    PlayerLogisticsRecord, PlayerStance, assess_player_carrying, horizontal_chunks_generated,
    horizontal_voxel_bounds, player_body_is_grounded, resolve_horizontal_control,
    resolve_vertical_control, supporting_floor_y,
};

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
    PlayerDead,
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
            Self::PlayerDead => formatter.write_str("dead player cannot project a walk"),
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
/// Both walking speed and locomotion exertion use the projected mass, so the caller can compare
/// planned return hauls without manufacturing inventory custody. The requested load must fit
/// the supplied body-carrying capacity. Route discovery remains caller-owned.
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
    let mut projection = WalkProjectionCursor {
        registries,
        state,
        body: player.body(),
        exertion: player.locomotion_exertion(),
        control_ticks: 0,
        maximum_control_ticks: CONTROL_TICKS_PER_SIMULATION_TICK - 1,
        carried,
        capacity,
    };

    for &destination in destinations {
        project_walk_leg(&mut projection, destination)?;
    }

    let locomotion_exertion = projection.exertion.pending();
    let resource_budget = project_survival_resource_budget(
        registries.survival().physiology(),
        locomotion_exertion,
        TickSpan::new(1),
    )
    .map_err(PlayerWalkProjectionError::ResourceProjection)?;
    Ok(PlayerWalkProjection {
        origin,
        destination,
        control_ticks: projection.control_ticks,
        locomotion_exertion,
        resource_budget,
    })
}

fn validate_walk_projection_start(
    registries: &Registries,
    state: &AppState,
) -> Result<PlayerLogisticsRecord, PlayerWalkProjectionError> {
    validate_player_action(state).map_err(|error| match error {
        PlayerActionValidationError::PlayerNotAdmitted => {
            PlayerWalkProjectionError::PlayerNotAdmitted
        }
        PlayerActionValidationError::PlayerDead => PlayerWalkProjectionError::PlayerDead,
    })?;
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

/// Ephemeral replay state shared by commanded movement and passive braking. Only commanded
/// horizontal motion accrues active exertion, matching the authoritative control owner.
struct WalkProjectionCursor<'a> {
    registries: &'a Registries,
    state: &'a AppState,
    body: PlayerBodyState,
    exertion: LocomotionExertionAccumulator,
    control_ticks: u64,
    maximum_control_ticks: u64,
    carried: Mass,
    capacity: Mass,
}

fn project_walk_leg(
    projection: &mut WalkProjectionCursor<'_>,
    destination: VoxelCoord,
) -> Result<(), PlayerWalkProjectionError> {
    let leg_origin = projection.body.voxel_position();
    if !leg_origin.is_face_adjacent_to(destination) || leg_origin.y() != destination.y() {
        return Err(
            PlayerWalkProjectionError::DestinationNotHorizontalAdjacent {
                origin: leg_origin,
                destination,
            },
        );
    }
    let movement = adjacent_walk_input(leg_origin, destination);
    while projection.body.voxel_position() != destination {
        ensure_walk_tick_available(
            projection.control_ticks,
            projection.maximum_control_ticks,
            leg_origin,
            destination,
        )?;
        let before = projection.body;
        let after = project_player_walk_body_tick(
            projection.registries,
            projection.state,
            before,
            movement,
            projection.carried,
            projection.capacity,
        )
        .map_err(PlayerWalkProjectionError::BodyMotion)?;
        if after == before {
            return Err(
                PlayerWalkProjectionError::DestinationUnreachableBeforeBoundary {
                    origin: leg_origin,
                    destination,
                },
            );
        }
        projection.body = after;
        accrue_projected_walk_exertion(
            projection.registries,
            before,
            after,
            projection.carried,
            projection.capacity,
            &mut projection.exertion,
        )?;
        projection.control_ticks += 1;
        let actual = projection.body.voxel_position();
        if actual != leg_origin && actual != destination {
            return Err(PlayerWalkProjectionError::DestinationOvershot {
                destination,
                actual,
            });
        }
    }
    settle_projected_walk_velocity(projection, leg_origin, destination)
}

fn accrue_projected_walk_exertion(
    registries: &Registries,
    before: PlayerBodyState,
    after: PlayerBodyState,
    carried: Mass,
    capacity: Mass,
    accumulator: &mut LocomotionExertionAccumulator,
) -> Result<(), PlayerWalkProjectionError> {
    let Some(exertion) = walking_exertion(
        registries.survival().physiology().locomotion(),
        before.foot_position(),
        after.foot_position(),
        carried,
        capacity,
    )
    .map_err(PlayerWalkProjectionError::BodyMotion)?
    else {
        return Ok(());
    };
    *accumulator =
        accumulator
            .checked_accrue(exertion)
            .ok_or(PlayerWalkProjectionError::BodyMotion(
                PlayerBodyMotionError::ExertionOverflow,
            ))?;
    Ok(())
}

fn settle_projected_walk_velocity(
    projection: &mut WalkProjectionCursor<'_>,
    leg_origin: VoxelCoord,
    destination: VoxelCoord,
) -> Result<(), PlayerWalkProjectionError> {
    loop {
        let body = projection.body;
        let horizontal = body.horizontal_velocity();
        if horizontal.x_nanometers_per_second() == 0 && horizontal.z_nanometers_per_second() == 0 {
            return Ok(());
        }
        ensure_walk_tick_available(
            projection.control_ticks,
            projection.maximum_control_ticks,
            leg_origin,
            destination,
        )?;
        let after = project_player_walk_body_tick(
            projection.registries,
            projection.state,
            body,
            PlayerControlInput::NEUTRAL,
            projection.carried,
            projection.capacity,
        )
        .map_err(PlayerWalkProjectionError::BodyMotion)?;
        projection.body = after;
        // The authoritative control owner does not charge locomotion for neutral-input
        // braking, even when inertia displaces the body. Replay the motion but not exertion.
        projection.control_ticks += 1;
        let actual = projection.body.voxel_position();
        if actual != destination {
            return Err(PlayerWalkProjectionError::DestinationOvershot {
                destination,
                actual,
            });
        }
    }
}

fn project_player_walk_body_tick(
    registries: &Registries,
    state: &AppState,
    before: PlayerBodyState,
    input: PlayerControlInput,
    carried: Mass,
    capacity: Mass,
) -> Result<PlayerBodyState, PlayerBodyMotionError> {
    let locomotion = registries.survival().physiology().locomotion();
    let bounds = horizontal_voxel_bounds(before, locomotion)
        .ok_or(PlayerBodyMotionError::CoordinateOverflow)?;
    if !horizontal_chunks_generated(state.world(), bounds) {
        return Ok(before);
    }
    let availability = PlayerControlAvailability {
        alive: state
            .survival()
            .player()
            .is_some_and(|survival| survival.vitality() != Vitality::ZERO),
        busy: false,
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
        Some((carried, capacity)),
    )?;
    let (after, _, _) = resolve_vertical_control(
        registries,
        state,
        horizontal_body,
        locomotion,
        availability,
        input,
    )?;
    Ok(after)
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
