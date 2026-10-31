//! Scout-local interpretation of the shared two-cell terrain sightline.

use crate::core::state::AppState;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;

pub(super) use crate::world::within_manhattan_reach;

/// A resident and the player use the same physical wall/tree opacity.
pub(super) fn scout_visible_from(
    registries: &Registries,
    state: &AppState,
    scout: VoxelCoord,
    observer: VoxelCoord,
) -> bool {
    crate::world::has_clear_two_step_sightline(registries, state, scout, observer)
}
