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

/// Player-facing observation of a physically extended resident.
///
/// Every body cell competes through the same actual world illumination and
/// occlusion check. A visible head must not be lost behind opaque feet, and an
/// unseen lower body must not grant a whole-caste sighting in darkness.
/// Resident-to-resident awareness uses separate geometric sight instead.
pub(super) fn player_sees_resident_body(
    registries: &Registries,
    state: &AppState,
    player: VoxelCoord,
    body_cells: impl IntoIterator<Item = VoxelCoord>,
) -> bool {
    let Some(head_y) = player.y().checked_add(1) else {
        return false;
    };
    let player_head = VoxelCoord::new(player.x(), head_y, player.z());
    body_cells.into_iter().any(|cell| {
        [player, player_head]
            .into_iter()
            .any(|eye| crate::world::player_can_see_local_cell(registries, state, eye, cell))
    })
}
