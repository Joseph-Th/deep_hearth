//! Actor-local physical clues to exposed copper-bearing geological matter.

use crate::content::{FORM_ORE, MATERIAL_COPPER};
use crate::core::state::AppState;
use crate::material::CommodityKey;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::survival::Vitality;

/// Visible stained rubble, not an assay or mining authorization. Only single
/// voxel copper bodies in open working space can be recognized from the surface.
#[must_use]
pub fn visible_weathered_copper_sites(
    registries: &Registries,
    state: &AppState,
) -> Vec<VoxelCoord> {
    let Some(from) = state.player_observation_position() else {
        return Vec::new();
    };
    if !state
        .survival()
        .player()
        .is_some_and(|player| player.vitality() != Vitality::ZERO)
        || !state.world().has_generated_terrain_at(from)
    {
        return Vec::new();
    }
    let mut sites = state
        .geology()
        .deposits()
        .filter_map(|deposit| {
            let bounds = deposit.bounds();
            let to = bounds.min();
            let max = bounds.max_exclusive();
            (deposit.commodity() == CommodityKey::new(MATERIAL_COPPER, FORM_ORE)
                && !deposit.remaining_mass().is_zero()
                && max.x().abs_diff(to.x()) == 1
                && max.y().abs_diff(to.y()) == 1
                && max.z().abs_diff(to.z()) == 1
                && from.x().abs_diff(to.x()) <= 12
                && from.y().abs_diff(to.y()) <= 6
                && from.z().abs_diff(to.z()) <= 12
                && state.world().has_standing_clearance(registries.world(), to)
                && !state.structures().blocks_full_cell(to)
                && crate::world::player_can_see_distant_cell(registries, state, to))
            .then_some(to)
        })
        .collect::<Vec<_>>();
    sites.sort_unstable();
    sites.dedup();
    sites
}
