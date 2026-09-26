//! Regional abundance projection and exact voxel-coverage geometry for field prospecting.

use crate::core::arithmetic::NORMALIZED_PARTS_PER_MILLION;
use crate::core::state::AppState;
use crate::material::MaterialId;
use crate::spatial::VoxelBounds;

use super::super::GeologicalDepositLifecycle;
use super::super::coverage::VoxelCoverage;

pub(super) fn resolve_region_abundance_bounds(
    state: &AppState,
    region: VoxelBounds,
    material: MaterialId,
    uncertainty_ppm: u32,
) -> (u32, u32) {
    // Live-truth projection: only available deposits hold extractable matter, so depleted bodies
    // are excluded and their former footprint reads as uncovered (zero lower bound). A collapsing
    // re-survey after the player's own extraction is intended observable feedback, not a hidden
    // lifecycle side channel.
    let mut minimum = None::<u32>;
    let mut maximum = None::<u32>;
    let mut coverage = VoxelCoverage::new(region);
    for deposit in state.geology().deposits().filter(|deposit| {
        deposit.lifecycle() == GeologicalDepositLifecycle::Available
            && deposit.bounds().has_intersection(region)
    }) {
        let abundance = deposit.composition().parts_per_million(material);
        minimum = Some(minimum.map_or(abundance, |current| current.min(abundance)));
        maximum = Some(maximum.map_or(abundance, |current| current.max(abundance)));
        coverage.cover(deposit.bounds());
    }
    let minimum = if coverage.is_complete() {
        minimum.unwrap_or(0)
    } else {
        0
    };
    let maximum = maximum.unwrap_or(0);
    (
        minimum.saturating_sub(uncertainty_ppm),
        maximum
            .saturating_add(uncertainty_ppm)
            .min(NORMALIZED_PARTS_PER_MILLION),
    )
}
