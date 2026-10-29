//! Local-only scout observation through authoritative voxel opacity.

use crate::core::state::AppState;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;

/// Bounded local reach without overflow at the extremes of checked world coordinates.
/// A body that has moved away from its home must still respond to nearby observers.
pub(super) fn within_manhattan_reach(from: VoxelCoord, to: VoxelCoord, reach: u64) -> bool {
    let Some(remaining) = reach.checked_sub(from.x().abs_diff(to.x())) else {
        return false;
    };
    let Some(remaining) = remaining.checked_sub(from.y().abs_diff(to.y())) else {
        return false;
    };
    remaining >= from.z().abs_diff(to.z())
}

/// Close-range scout sightings use authoritative terrain and built-member occlusion, not distance alone.
/// At two voxels, all intermediate axis steps must be transparent; a diagonal
/// corner is concealed by a blocking wall on either side. Missing terrain
/// fails closed when either endpoint has generated terrain authority; purely
/// ungenerated authored graphs have no terrain opacity. Constructed full-cell
/// members block either kind of local sightline.
pub(super) fn scout_visible_from(
    registries: &Registries,
    state: &AppState,
    scout: VoxelCoord,
    observer: VoxelCoord,
) -> bool {
    if !within_manhattan_reach(scout, observer, 2) {
        return false;
    }
    let generated_at_endpoint = [scout, observer]
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
    if !transparent(scout) || !transparent(observer) {
        return false;
    }
    if within_manhattan_reach(scout, observer, 1) {
        return true;
    }
    let toward = |first: i64, second: i64| {
        if first < second {
            first.checked_add(1)
        } else {
            first.checked_sub(1)
        }
    };
    (scout.x() == observer.x()
        || toward(scout.x(), observer.x())
            .is_some_and(|x| transparent(VoxelCoord::new(x, scout.y(), scout.z()))))
        && (scout.y() == observer.y()
            || toward(scout.y(), observer.y())
                .is_some_and(|y| transparent(VoxelCoord::new(scout.x(), y, scout.z()))))
        && (scout.z() == observer.z()
            || toward(scout.z(), observer.z())
                .is_some_and(|z| transparent(VoxelCoord::new(scout.x(), scout.y(), z))))
}
