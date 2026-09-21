//! Actor-safe resource-scale projection for fully localized physical geological sampling.

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::material::MaterialId;
use crate::spatial::VoxelBounds;

use super::super::{GeologicalDepositLifecycle, ResourceMassEstimate};

fn contains_bounds(outer: VoxelBounds, inner: VoxelBounds) -> bool {
    let outer_min = outer.min();
    let outer_max = outer.max_exclusive();
    let inner_min = inner.min();
    let inner_max = inner.max_exclusive();
    outer_min.x() <= inner_min.x()
        && outer_min.y() <= inner_min.y()
        && outer_min.z() <= inner_min.z()
        && outer_max.x() >= inner_max.x()
        && outer_max.y() >= inner_max.y()
        && outer_max.z() >= inner_max.z()
}

pub(super) fn resolve_region_resource_mass(
    state: &AppState,
    region: VoxelBounds,
    material: MaterialId,
    resolution: Mass,
) -> Option<ResourceMassEstimate> {
    debug_assert!(!resolution.is_zero());
    let mut matching = state.geology().deposits().filter(|deposit| {
        deposit.lifecycle() == GeologicalDepositLifecycle::Available
            && deposit.bounds().has_intersection(region)
            && deposit.composition().parts_per_million(material) > 0
    });
    let deposit = matching.next()?;
    if matching.next().is_some() || !contains_bounds(region, deposit.bounds()) {
        return None;
    }
    let actual = deposit.remaining_mass();
    let resolution_mg = resolution.milligrams();
    let actual_mg = actual.milligrams();
    let lower_mg = actual_mg
        .checked_div(resolution_mg)
        .unwrap_or_else(|| unreachable!("nonzero resource-mass resolution divides mass"))
        .checked_mul(resolution_mg)
        .unwrap_or_else(|| unreachable!("resource-mass lower bucket cannot exceed input"));
    // Keep exact bucket boundaries inside a nonzero-width acquired band. A finite-resolution
    // physical observation must not reveal hidden reserve exactly merely because truth happens to
    // align with the instrument's bucket edge.
    let upper_mg = actual_mg
        .checked_div(resolution_mg)
        .and_then(|bucket| bucket.checked_add(1))
        .and_then(|bucket| bucket.checked_mul(resolution_mg))
        .unwrap_or(u64::MAX);
    ResourceMassEstimate::new(
        Mass::from_milligrams(lower_mg),
        Mass::from_milligrams(upper_mg),
    )
    .ok()
}

#[cfg(test)]
#[path = "resource_mass_tests.rs"]
mod tests;
