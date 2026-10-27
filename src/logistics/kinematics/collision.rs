//! Shared continuous-player collision authority, occupied bounds and world generation gates.

use super::{
    AppState, LocomotionDefinition, NANOMETERS_PER_MICROMETER, PlayerBodyState, PlayerStance,
    Registries, supporting_floor_y,
};
use crate::spatial::{NANOMETERS_PER_VOXEL, VoxelBounds, VoxelCoord};
use crate::world::{
    BlockCollisionShape, ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y, WorldChunk, WorldRegistry,
    WorldState,
};

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

/// A materialized full-cell member beneath any part of the player's horizontal
/// footprint can be the current floor. Its removal at a slow boundary must
/// retain the immediately following 20 Hz gravity/collision step.
pub(crate) fn player_body_stands_above_structure(
    registries: &Registries,
    state: &AppState,
    element: crate::structural::StructuralElementId,
) -> bool {
    let Some(record) = state.structures().get_element(element) else {
        return false;
    };
    if record.embodied_material().is_empty() || !record.geometry().covers_horizontal_footprint() {
        return false;
    }
    let Some(player) = state.logistics().player() else {
        return false;
    };
    let Some((min_x, max_x, min_z, max_z)) = horizontal_voxel_bounds(
        player.body(),
        registries.survival().physiology().locomotion(),
    ) else {
        return true;
    };
    let Some(below_y) = player.body().foot_position().y_nanometers().checked_sub(1) else {
        return true;
    };
    let below_y = below_y.div_euclid(NANOMETERS_PER_VOXEL);
    let bounds = record.bounds();
    let min = bounds.min();
    let max = bounds.max_exclusive();
    min.y() <= below_y
        && below_y < max.y()
        && min.x() <= max_x
        && max.x() > min_x
        && min.z() <= max_z
        && max.z() > min_z
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
    state.world().generation_profile()?;
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
    state.world().generation_profile()?;
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

pub(super) fn first_player_body_collision_where(
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

pub(super) const fn body_height(
    locomotion: LocomotionDefinition,
    stance: PlayerStance,
) -> crate::core::quantity::Length {
    match stance {
        PlayerStance::Standing => locomotion.standing_height(),
        PlayerStance::Crouching => locomotion.crouching_height(),
    }
}

/// All movement axes, standing clearance and grounding share solid-cell authority.
/// Concealed temple residents remain physical obstacles just like generated rock.
/// Small opening residents are contact targets rather than full-cube walls:
/// same-cell melee and den-custody recovery must remain possible. Their patrol
/// separately avoids entering the larger player's occupied footprint.
pub(super) fn player_cell_blocks_motion(
    registries: &Registries,
    state: &AppState,
    voxel: VoxelCoord,
) -> bool {
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

pub(super) fn horizontal_voxel_bounds(
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

pub(super) fn horizontal_chunks_generated(
    world: &WorldState,
    bounds: (i64, i64, i64, i64),
) -> bool {
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

pub(super) fn length_nanometers(length: crate::core::quantity::Length) -> Option<i64> {
    i64::try_from(u128::from(length.micrometers()).checked_mul(NANOMETERS_PER_MICROMETER as u128)?)
        .ok()
}
