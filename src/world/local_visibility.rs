//! Shared short-range physical visibility for residents and actor observations.
//!
//! The world owns terrain/tree occlusion. A player-local observation and a scout
//! should not disagree about whether an adjacent wall conceals a physical cell.

use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;

/// Checked local reach without signed coordinate arithmetic overflow.
pub(crate) fn within_manhattan_reach(from: VoxelCoord, to: VoxelCoord, reach: u64) -> bool {
    let Some(remaining) = reach.checked_sub(from.x().abs_diff(to.x())) else {
        return false;
    };
    let Some(remaining) = remaining.checked_sub(from.y().abs_diff(to.y())) else {
        return false;
    };
    remaining >= from.z().abs_diff(to.z())
}

/// At most two voxel steps, with every intermediate cell transparent. A
/// diagonal corner is hidden if either side obstructs sight. Missing terrain
/// cannot be seen through when either endpoint belongs to a generated chunk;
/// ungenerated authored routes retain their original local visibility rules.
pub(crate) fn has_clear_two_step_sightline(
    registries: &Registries,
    state: &AppState,
    from: VoxelCoord,
    to: VoxelCoord,
) -> bool {
    if !within_manhattan_reach(from, to, 2) {
        return false;
    }
    let generated_at_endpoint = [from, to]
        .into_iter()
        .any(|position| state.world().has_generated_terrain_at(position));
    let transparent = |voxel| {
        !state.structures().blocks_full_cell(voxel)
            && state.world().tree_trunk_at(voxel).is_none()
            && match state.world().block_at(voxel) {
                Some(id) => registries
                    .world()
                    .block_for_state(id)
                    .is_some_and(|block| !block.is_opaque()),
                None => !generated_at_endpoint,
            }
    };
    if !transparent(from) || !transparent(to) {
        return false;
    }
    if within_manhattan_reach(from, to, 1) {
        return true;
    }
    let toward = |first: i64, second: i64| {
        if first < second {
            first.checked_add(1)
        } else {
            first.checked_sub(1)
        }
    };
    (from.x() == to.x()
        || toward(from.x(), to.x())
            .is_some_and(|x| transparent(VoxelCoord::new(x, from.y(), from.z()))))
        && (from.y() == to.y()
            || toward(from.y(), to.y())
                .is_some_and(|y| transparent(VoxelCoord::new(from.x(), y, from.z()))))
        && (from.z() == to.z()
            || toward(from.z(), to.z())
                .is_some_and(|z| transparent(VoxelCoord::new(from.x(), from.y(), z))))
}

/// Every local visual probe uses the same 25-cell Manhattan envelope.
/// Checked coordinates omit out-of-range neighbors rather than wrapping.
pub(crate) fn nearby_two_step_cells(center: VoxelCoord) -> impl Iterator<Item = VoxelCoord> {
    (-2_i64..=2).flat_map(move |dx| {
        let remaining = 2 - dx.abs();
        (-remaining..=remaining).flat_map(move |dy| {
            let last = remaining - dy.abs();
            (-last..=last).filter_map(move |dz| {
                Some(VoxelCoord::new(
                    center.x().checked_add(dx)?,
                    center.y().checked_add(dy)?,
                    center.z().checked_add(dz)?,
                ))
            })
        })
    })
}

/// Lit visibility between two explicit voxel cells, independent of actor pose.
///
/// The generated world has no automatic night vision. Ungenerated authored
/// routes retain geometric observation because they have no sky. For player
/// observations, prefer `player_can_see_from_eye` instead of treating the
/// character's feet as an additional viewpoint.
/// Event observation uses the incoming tick's skylight before the root clock
/// advances. Occlusion and artificial illumination still read committed owners.
pub(crate) fn player_can_see_local_cell_at(
    registries: &Registries,
    state: &AppState,
    observer: VoxelCoord,
    target: VoxelCoord,
    at: SimulationTick,
) -> bool {
    has_clear_two_step_sightline(registries, state, observer, target)
        && ((!state.world().has_generated_terrain_at(observer)
            && !state.world().has_generated_terrain_at(target))
            || super::sky::assess_generated_skylight_at(registries, state, target, at) > 0
            || crate::energy::assess_local_illumination(registries, state, target).level() > 0)
}

/// A player-facing observation starts at the actual stance-dependent eye.
///
/// A slightly downward view of ground-level matter can cross the horizontal
/// two-cell envelope even when its Manhattan distance from the eye is three.
/// Both height corridors must be clear: a low opening beneath a solid lintel
/// cannot act as an additional eye at the player's feet.
pub(crate) fn player_can_see_from_eye_at(
    registries: &Registries,
    state: &AppState,
    player: VoxelCoord,
    target: VoxelCoord,
    at: SimulationTick,
) -> bool {
    if state
        .logistics()
        .player()
        .is_none_or(|record| record.position() != player)
    {
        return false;
    }
    let Some(eye) = super::player_eye_voxel(registries, state) else {
        return false;
    };
    if player_can_see_local_cell_at(registries, state, eye, target, at) {
        return true;
    }
    // A downward view splits at one real intermediate height, rather than
    // borrowing an independent view from the feet. When the player stands one
    // voxel above the target (for example, overlooking a den), the same
    // two-corridor requirement applies to the steeper downward sightline.
    let bridge_y = match eye.y().checked_sub(target.y()) {
        Some(1) if player.y() == target.y() => eye.y(),
        Some(2) if player.y().checked_sub(target.y()) == Some(1) => player.y(),
        _ => return false,
    };
    let bridge = VoxelCoord::new(target.x(), bridge_y, target.z());
    has_clear_two_step_sightline(registries, state, eye, bridge)
        && player_can_see_local_cell_at(registries, state, player, target, at)
}

/// The same eye-based observation at the committed simulation tick.
pub(crate) fn player_can_see_from_eye(
    registries: &Registries,
    state: &AppState,
    player: VoxelCoord,
    target: VoxelCoord,
) -> bool {
    player_can_see_from_eye_at(registries, state, player, target, state.tick())
}

#[cfg(test)]
#[path = "local_visibility_tests.rs"]
mod tests;
