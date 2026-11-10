//! Control-rate body and survival replay for an explicit actor-chosen walking path.

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::{CONTROL_TICKS_PER_SIMULATION_TICK, TickSpan};
use crate::player::{PlayerActionValidationError, validate_player_action};
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::survival::{SurvivalExertion, Vitality, project_survival_resource_budget};

use super::super::exertion::walking_exertion;
use super::super::{
    CONTROL_INPUT_PARTS_PER_MILLION, HorizontalControlInput, LocomotionExertionAccumulator,
    PlayerBodyMotionError, PlayerBodyState, PlayerControlAvailability, PlayerControlInput,
    PlayerLogisticsRecord, PlayerStance, horizontal_chunks_generated, horizontal_voxel_bounds,
    player_body_is_grounded, resolve_horizontal_control, resolve_vertical_control,
    supporting_floor_y,
};
use super::{PlayerWalkProjection, PlayerWalkProjectionError};

/// The two public planning forms share one exact body, timing, and survival
/// projection without recomputing carried equipment for the current-load form.
pub(super) fn project_walk_path_with_load(
    registries: &Registries,
    state: &AppState,
    destinations: &[VoxelCoord],
    carried: Mass,
    capacity: Mass,
) -> Result<PlayerWalkProjection, PlayerWalkProjectionError> {
    let destination = *destinations
        .last()
        .ok_or(PlayerWalkProjectionError::PathEmpty)?;
    // Actor liveness, footing, and work readiness precede a hypothetical
    // hauling limit: a dead or occupied actor cannot request a walk forecast.
    let player = validate_walk_projection_start(registries, state)?;
    // The admitted physiology owns a strictly positive body envelope. Portable
    // equipment reduces free material space, not total body carrying capacity.
    if carried > capacity {
        return Err(PlayerWalkProjectionError::ProjectedLoadExceedsCapacity { carried, capacity });
    }
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

    for (index, &destination) in destinations.iter().enumerate() {
        let leg_origin = projection.body.voxel_position();
        let continues_straight = destinations.get(index + 1).is_some_and(|&next| {
            // Verify both edges before subtracting coordinates. Face-adjacent
            // differences fit i64 even near the limits of the world grid.
            leg_origin.is_face_adjacent_to(destination)
                && leg_origin.y() == destination.y()
                && destination.is_face_adjacent_to(next)
                && destination.y() == next.y()
                && destination.x() - leg_origin.x() == next.x() - destination.x()
                && destination.z() - leg_origin.z() == next.z() - destination.z()
        });
        // Braking at every voxel on a straight path invents extra stops,
        // travel time and repeated acceleration. Keep motion continuous until
        // a real change of heading or the final requested destination.
        project_walk_leg(&mut projection, destination, !continues_straight)?;
    }
    // Each projected control sample changes the persisted body or its velocity,
    // consuming one logistics revision on execution. Check the whole route and
    // already-promised future logistics work before advertising its budget.
    if !state.can_spend_logistics_revisions(projection.control_ticks) {
        return Err(PlayerWalkProjectionError::BodyMotion(
            PlayerBodyMotionError::LogisticsRevisionExhausted,
        ));
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
    settle_at_destination: bool,
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
    if settle_at_destination {
        settle_projected_walk_velocity(projection, leg_origin, destination)
    } else {
        Ok(())
    }
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
