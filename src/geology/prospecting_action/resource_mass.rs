//! Actor-safe resource-scale projection for fully localized physical geological sampling.

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::material::MaterialId;
use crate::spatial::VoxelBounds;

use super::super::{GeologicalDepositLifecycle, ResourceMassEstimate};

fn resource_mass_bucket(actual: Mass, resolution: Mass) -> ResourceMassEstimate {
    let resolution_mg = resolution.milligrams();
    assert!(
        resolution_mg > 0,
        "validated resource-mass sampling resolution must be nonzero"
    );
    let actual_mg = actual.milligrams();
    let mut lower_mg = (actual_mg / resolution_mg)
        .checked_mul(resolution_mg)
        .unwrap_or_else(|| unreachable!("resource-mass lower bucket cannot exceed input"));
    let upper_mg = (actual_mg / resolution_mg)
        .checked_add(1)
        .and_then(|bucket| bucket.checked_mul(resolution_mg))
        .unwrap_or(u64::MAX);

    // At the representational ceiling there may be no larger upper bucket. Move the lower bound
    // down by one instrument resolution instead of collapsing the observation to exact hidden
    // reserve state.
    if lower_mg == upper_mg {
        lower_mg = lower_mg.checked_sub(resolution_mg).unwrap_or_else(|| {
            unreachable!("equal nonzero resource-mass bounds contain one full resolution")
        });
    }

    ResourceMassEstimate::new(
        Mass::from_milligrams(lower_mg),
        Mass::from_milligrams(upper_mg),
    )
    .unwrap_or_else(|error| panic!("validated resource-mass bucket is invalid: {error}"))
}

pub(super) fn resolve_region_resource_mass(
    state: &AppState,
    region: VoxelBounds,
    material: MaterialId,
    resolution: Mass,
) -> Option<ResourceMassEstimate> {
    let mut matching = state.geology().deposits().filter(|deposit| {
        deposit.lifecycle() == GeologicalDepositLifecycle::Available
            && deposit.bounds().has_intersection(region)
            && deposit.composition().parts_per_million(material) > 0
    });
    let deposit = matching.next()?;
    if matching.next().is_some() || region != deposit.bounds() {
        return None;
    }
    Some(resource_mass_bucket(deposit.remaining_mass(), resolution))
}

#[cfg(test)]
#[path = "resource_mass_tests.rs"]
mod tests;
