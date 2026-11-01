//! Shared short-range physical visibility for residents and actor observations.
//!
//! The world owns terrain/tree occlusion. A player-local observation and a scout
//! should not disagree about whether an adjacent wall conceals a physical cell.

use crate::core::state::AppState;
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

/// One visual rule shared by player-facing water, forage, and resident observations.
///
/// The generated world has no automatic night vision. Physical line of sight
/// alone cannot reveal an unlit cell. Ungenerated authored routes retain
/// their existing geometric observation contract because they have no sky.
pub(crate) fn player_can_see_local_cell(
    registries: &Registries,
    state: &AppState,
    observer: VoxelCoord,
    target: VoxelCoord,
) -> bool {
    has_clear_two_step_sightline(registries, state, observer, target)
        && ((!state.world().has_generated_terrain_at(observer)
            && !state.world().has_generated_terrain_at(target))
            || super::assess_generated_skylight(registries, state, target) > 0
            || crate::energy::assess_local_illumination(registries, state, target).level() > 0)
}

#[cfg(test)]
#[path = "local_visibility_tests.rs"]
mod tests;
