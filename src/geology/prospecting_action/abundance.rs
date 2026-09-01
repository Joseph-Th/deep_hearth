//! Regional abundance projection and exact voxel-coverage geometry for field prospecting.

use crate::core::state::AppState;
use crate::material::MaterialId;
use crate::spatial::{VoxelBounds, VoxelCoord};

use super::super::GeologicalDepositLifecycle;

pub(super) fn resolve_region_abundance_bounds(
    state: &AppState,
    region: VoxelBounds,
    material: MaterialId,
    uncertainty_ppm: u32,
) -> (u32, u32) {
    let mut minimum = None::<u32>;
    let mut maximum = None::<u32>;
    let mut uncovered = vec![region];
    for deposit in state.geology().deposits().filter(|deposit| {
        deposit.lifecycle() == GeologicalDepositLifecycle::Available
            && deposit.bounds().has_intersection(region)
    }) {
        let abundance = deposit.composition().parts_per_million(material);
        minimum = Some(minimum.map_or(abundance, |current| current.min(abundance)));
        maximum = Some(maximum.map_or(abundance, |current| current.max(abundance)));
        if !uncovered.is_empty() {
            uncovered = uncovered
                .into_iter()
                .flat_map(|bounds| subtract_bounds(bounds, deposit.bounds()))
                .collect();
        }
    }
    let minimum = if uncovered.is_empty() {
        minimum.unwrap_or(0)
    } else {
        0
    };
    let maximum = maximum.unwrap_or(0);
    (
        minimum.saturating_sub(uncertainty_ppm),
        maximum.saturating_add(uncertainty_ppm).min(1_000_000),
    )
}

fn subtract_bounds(bounds: VoxelBounds, cover: VoxelBounds) -> Vec<VoxelBounds> {
    let Some(overlap) = bounds.intersection(cover) else {
        return vec![bounds];
    };
    let min = bounds.min();
    let max = bounds.max_exclusive();
    let overlap_min = overlap.min();
    let overlap_max = overlap.max_exclusive();
    let mut remainder = Vec::with_capacity(6);

    push_bounds(
        &mut remainder,
        VoxelCoord::new(min.x(), min.y(), min.z()),
        VoxelCoord::new(overlap_min.x(), max.y(), max.z()),
    );
    push_bounds(
        &mut remainder,
        VoxelCoord::new(overlap_max.x(), min.y(), min.z()),
        VoxelCoord::new(max.x(), max.y(), max.z()),
    );
    push_bounds(
        &mut remainder,
        VoxelCoord::new(overlap_min.x(), min.y(), min.z()),
        VoxelCoord::new(overlap_max.x(), overlap_min.y(), max.z()),
    );
    push_bounds(
        &mut remainder,
        VoxelCoord::new(overlap_min.x(), overlap_max.y(), min.z()),
        VoxelCoord::new(overlap_max.x(), max.y(), max.z()),
    );
    push_bounds(
        &mut remainder,
        VoxelCoord::new(overlap_min.x(), overlap_min.y(), min.z()),
        VoxelCoord::new(overlap_max.x(), overlap_max.y(), overlap_min.z()),
    );
    push_bounds(
        &mut remainder,
        VoxelCoord::new(overlap_min.x(), overlap_min.y(), overlap_max.z()),
        VoxelCoord::new(overlap_max.x(), overlap_max.y(), max.z()),
    );
    remainder
}

fn push_bounds(remainder: &mut Vec<VoxelBounds>, min: VoxelCoord, max: VoxelCoord) {
    if min.x() >= max.x() || min.y() >= max.y() || min.z() >= max.z() {
        return;
    }
    remainder.push(
        VoxelBounds::new(min, max)
            .unwrap_or_else(|_| unreachable!("positive prospecting remainder bounds are valid")),
    );
}
