//! Scout-local interpretation of the shared two-cell terrain sightline.

use crate::core::state::AppState;
use crate::core::time::SimulationTick;
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
    player_sees_resident_body_at(registries, state, player, body_cells, state.tick())
}

/// Opening residents occupy two real vertical cells. The same head-aware
/// player sight rule used for temple castes must apply to a visible scout or
/// watchman, without granting sight of ground-level goods behind a short wall.
pub(super) fn player_sees_opening_resident_at(
    registries: &Registries,
    state: &AppState,
    player: VoxelCoord,
    resident: VoxelCoord,
    at: SimulationTick,
) -> bool {
    let Some(head_y) = resident.y().checked_add(1) else {
        return false;
    };
    player_sees_resident_body_at(
        registries,
        state,
        player,
        [
            resident,
            VoxelCoord::new(resident.x(), head_y, resident.z()),
        ],
        at,
    )
}

/// Event receipts use the incoming slow tick's daylight, while read-only HUD
/// sightings use the same physical body test with the committed state tick.
fn player_sees_resident_body_at(
    registries: &Registries,
    state: &AppState,
    player: VoxelCoord,
    body_cells: impl IntoIterator<Item = VoxelCoord>,
    at: SimulationTick,
) -> bool {
    let Some(head_y) = player.y().checked_add(1) else {
        return false;
    };
    let player_head = VoxelCoord::new(player.x(), head_y, player.z());
    body_cells.into_iter().any(|cell| {
        [player, player_head]
            .into_iter()
            .any(|eye| crate::world::player_can_see_local_cell_at(registries, state, eye, cell, at))
    })
}
