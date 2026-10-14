//! Horizontal body sweep and locality-transition constraints.

use crate::core::state::AppState;
use crate::equipment::equipment_occupancy;
use crate::production::player_move_production_site_conflict;
use crate::spatial::{HorizontalVelocity, NANOMETERS_PER_VOXEL, VoxelCoord, WorldPoint};
use crate::survival::LocomotionDefinition;
use crate::world::{ChunkCoord, WorldRegistry, WorldState};

use crate::logistics::{PlayerBodyState, PlayerLogisticsRecord};

use super::{
    PlayerBodyMotionError, block_is_solid, body_height, horizontal_voxel_bounds,
    integrate_velocity, length_nanometers,
};

pub(super) fn resolve_horizontal_motion(
    registry: &WorldRegistry,
    world: &WorldState,
    body: PlayerBodyState,
    locomotion: LocomotionDefinition,
    velocity: HorizontalVelocity,
) -> Result<(WorldPoint, bool, bool), PlayerBodyMotionError> {
    let position = body.foot_position();
    let proposed_x = position
        .x_nanometers()
        .checked_add(integrate_velocity(velocity.x_nanometers_per_second())?)
        .ok_or(PlayerBodyMotionError::CoordinateOverflow)?;
    let proposed_z = position
        .z_nanometers()
        .checked_add(integrate_velocity(velocity.z_nanometers_per_second())?)
        .ok_or(PlayerBodyMotionError::CoordinateOverflow)?;
    let (resolved_x, collided_x) =
        sweep_horizontal_x(registry, world, body, locomotion, proposed_x)?;
    let x_resolved = body.with_motion(
        WorldPoint::new(resolved_x, position.y_nanometers(), position.z_nanometers()),
        velocity,
        body.vertical_velocity(),
        body.stance(),
    );
    let (resolved_z, collided_z) =
        sweep_horizontal_z(registry, world, x_resolved, locomotion, proposed_z)?;
    Ok((
        WorldPoint::new(resolved_x, position.y_nanometers(), resolved_z),
        collided_x,
        collided_z,
    ))
}

fn sweep_horizontal_x(
    registry: &WorldRegistry,
    world: &WorldState,
    body: PlayerBodyState,
    locomotion: LocomotionDefinition,
    proposed_x: i64,
) -> Result<(i64, bool), PlayerBodyMotionError> {
    let current_x = body.foot_position().x_nanometers();
    if proposed_x == current_x {
        return Ok((current_x, false));
    }
    let (lower, upper) = body_horizontal_extents(locomotion)?;
    let (min_y, max_y) = vertical_voxel_bounds(body, locomotion)?;
    let (_, _, min_z, max_z) = horizontal_voxel_bounds(body, locomotion)
        .ok_or(PlayerBodyMotionError::CoordinateOverflow)?;
    if proposed_x > current_x {
        let current_leading = current_x
            .checked_add(upper)
            .ok_or(PlayerBodyMotionError::CoordinateOverflow)?;
        let proposed_leading = proposed_x
            .checked_add(upper)
            .ok_or(PlayerBodyMotionError::CoordinateOverflow)?;
        let first = current_leading.div_euclid(NANOMETERS_PER_VOXEL);
        let last = proposed_leading
            .checked_sub(1)
            .ok_or(PlayerBodyMotionError::CoordinateOverflow)?
            .div_euclid(NANOMETERS_PER_VOXEL);
        for x in first..=last {
            if column_x_blocks(registry, world, x, min_y, max_y, min_z, max_z) {
                let wall = x
                    .checked_mul(NANOMETERS_PER_VOXEL)
                    .ok_or(PlayerBodyMotionError::CoordinateOverflow)?;
                return Ok((
                    wall.checked_sub(upper)
                        .ok_or(PlayerBodyMotionError::CoordinateOverflow)?,
                    true,
                ));
            }
        }
        return Ok((proposed_x, false));
    }

    let current_leading = current_x
        .checked_sub(lower)
        .ok_or(PlayerBodyMotionError::CoordinateOverflow)?;
    let proposed_leading = proposed_x
        .checked_sub(lower)
        .ok_or(PlayerBodyMotionError::CoordinateOverflow)?;
    let first = proposed_leading.div_euclid(NANOMETERS_PER_VOXEL);
    let last = current_leading
        .checked_sub(1)
        .ok_or(PlayerBodyMotionError::CoordinateOverflow)?
        .div_euclid(NANOMETERS_PER_VOXEL);
    for x in (first..=last).rev() {
        if column_x_blocks(registry, world, x, min_y, max_y, min_z, max_z) {
            let wall = x
                .checked_add(1)
                .and_then(|value| value.checked_mul(NANOMETERS_PER_VOXEL))
                .ok_or(PlayerBodyMotionError::CoordinateOverflow)?;
            return Ok((
                wall.checked_add(lower)
                    .ok_or(PlayerBodyMotionError::CoordinateOverflow)?,
                true,
            ));
        }
    }
    Ok((proposed_x, false))
}

fn sweep_horizontal_z(
    registry: &WorldRegistry,
    world: &WorldState,
    body: PlayerBodyState,
    locomotion: LocomotionDefinition,
    proposed_z: i64,
) -> Result<(i64, bool), PlayerBodyMotionError> {
    let current_z = body.foot_position().z_nanometers();
    if proposed_z == current_z {
        return Ok((current_z, false));
    }
    let (lower, upper) = body_horizontal_extents(locomotion)?;
    let (min_y, max_y) = vertical_voxel_bounds(body, locomotion)?;
    let (min_x, max_x, _, _) = horizontal_voxel_bounds(body, locomotion)
        .ok_or(PlayerBodyMotionError::CoordinateOverflow)?;
    if proposed_z > current_z {
        let current_leading = current_z
            .checked_add(upper)
            .ok_or(PlayerBodyMotionError::CoordinateOverflow)?;
        let proposed_leading = proposed_z
            .checked_add(upper)
            .ok_or(PlayerBodyMotionError::CoordinateOverflow)?;
        let first = current_leading.div_euclid(NANOMETERS_PER_VOXEL);
        let last = proposed_leading
            .checked_sub(1)
            .ok_or(PlayerBodyMotionError::CoordinateOverflow)?
            .div_euclid(NANOMETERS_PER_VOXEL);
        for z in first..=last {
            if column_z_blocks(registry, world, z, min_y, max_y, min_x, max_x) {
                let wall = z
                    .checked_mul(NANOMETERS_PER_VOXEL)
                    .ok_or(PlayerBodyMotionError::CoordinateOverflow)?;
                return Ok((
                    wall.checked_sub(upper)
                        .ok_or(PlayerBodyMotionError::CoordinateOverflow)?,
                    true,
                ));
            }
        }
        return Ok((proposed_z, false));
    }

    let current_leading = current_z
        .checked_sub(lower)
        .ok_or(PlayerBodyMotionError::CoordinateOverflow)?;
    let proposed_leading = proposed_z
        .checked_sub(lower)
        .ok_or(PlayerBodyMotionError::CoordinateOverflow)?;
    let first = proposed_leading.div_euclid(NANOMETERS_PER_VOXEL);
    let last = current_leading
        .checked_sub(1)
        .ok_or(PlayerBodyMotionError::CoordinateOverflow)?
        .div_euclid(NANOMETERS_PER_VOXEL);
    for z in (first..=last).rev() {
        if column_z_blocks(registry, world, z, min_y, max_y, min_x, max_x) {
            let wall = z
                .checked_add(1)
                .and_then(|value| value.checked_mul(NANOMETERS_PER_VOXEL))
                .ok_or(PlayerBodyMotionError::CoordinateOverflow)?;
            return Ok((
                wall.checked_add(lower)
                    .ok_or(PlayerBodyMotionError::CoordinateOverflow)?,
                true,
            ));
        }
    }
    Ok((proposed_z, false))
}

fn column_x_blocks(
    registry: &WorldRegistry,
    world: &WorldState,
    x: i64,
    min_y: i64,
    max_y: i64,
    min_z: i64,
    max_z: i64,
) -> bool {
    for y in min_y..=max_y {
        for z in min_z..=max_z {
            if cell_blocks_horizontal_motion(registry, world, VoxelCoord::new(x, y, z)) {
                return true;
            }
        }
    }
    false
}

fn column_z_blocks(
    registry: &WorldRegistry,
    world: &WorldState,
    z: i64,
    min_y: i64,
    max_y: i64,
    min_x: i64,
    max_x: i64,
) -> bool {
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            if cell_blocks_horizontal_motion(registry, world, VoxelCoord::new(x, y, z)) {
                return true;
            }
        }
    }
    false
}

fn cell_blocks_horizontal_motion(
    registry: &WorldRegistry,
    world: &WorldState,
    voxel: VoxelCoord,
) -> bool {
    if world.get_chunk(ChunkCoord::containing(voxel)).is_none() {
        return true;
    }
    match world.block_at(voxel) {
        Some(state) => block_is_solid(registry, state),
        None => true,
    }
}

fn body_horizontal_extents(
    locomotion: LocomotionDefinition,
) -> Result<(i64, i64), PlayerBodyMotionError> {
    let width = length_nanometers(locomotion.body_width())
        .ok_or(PlayerBodyMotionError::CoordinateOverflow)?;
    let lower = width / 2;
    Ok((lower, width - lower))
}

fn vertical_voxel_bounds(
    body: PlayerBodyState,
    locomotion: LocomotionDefinition,
) -> Result<(i64, i64), PlayerBodyMotionError> {
    let foot_y = body.foot_position().y_nanometers();
    let height = length_nanometers(body_height(locomotion, body.stance()))
        .ok_or(PlayerBodyMotionError::CoordinateOverflow)?;
    let top_exclusive = foot_y
        .checked_add(height)
        .ok_or(PlayerBodyMotionError::CoordinateOverflow)?;
    Ok((
        foot_y.div_euclid(NANOMETERS_PER_VOXEL),
        top_exclusive
            .checked_sub(1)
            .ok_or(PlayerBodyMotionError::CoordinateOverflow)?
            .div_euclid(NANOMETERS_PER_VOXEL),
    ))
}

pub(super) fn player_voxel_change_allowed(
    state: &AppState,
    player: PlayerLogisticsRecord,
    destination: VoxelCoord,
) -> bool {
    if state.player_work().active().is_some() {
        return false;
    }
    if player_move_production_site_conflict(state, player.carried_stockpile(), destination)
        .is_some()
    {
        return false;
    }
    if state
        .mining()
        .awaiting_settlement_destination_occupant(player.carried_stockpile())
        .is_some()
    {
        return false;
    }
    !state
        .logistics()
        .carried_equipment()
        .any(|equipment| equipment_occupancy(state, equipment).is_some())
}
