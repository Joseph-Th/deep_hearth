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
    let aligned_lower_mg = (actual_mg / resolution_mg)
        .checked_mul(resolution_mg)
        .unwrap_or_else(|| unreachable!("resource-mass lower bucket cannot exceed input"));
    let aligned_upper_mg = (actual_mg / resolution_mg)
        .checked_add(1)
        .and_then(|bucket| bucket.checked_mul(resolution_mg));
    let (lower_mg, upper_mg) = match aligned_upper_mg {
        Some(upper_mg) => (aligned_lower_mg, upper_mg),
        None => (
            u64::MAX.checked_sub(resolution_mg).unwrap_or_else(|| {
                unreachable!("represented mass range contains one sampling resolution")
            }),
            u64::MAX,
        ),
    };

    ResourceMassEstimate::new(
        Mass::from_milligrams(lower_mg),
        Mass::from_milligrams(upper_mg),
    )
    .unwrap_or_else(|error| panic!("validated resource-mass bucket is invalid: {error}"))
}

pub(in crate::geology) fn resource_mass_band_matches_resolution(
    resource_mass: ResourceMassEstimate,
    resolution: Mass,
) -> bool {
    let resolution_mg = resolution.milligrams();
    if resolution_mg == 0 {
        return false;
    }
    let lower_mg = resource_mass.lower().milligrams();
    let upper_mg = resource_mass.upper().milligrams();
    if upper_mg == u64::MAX {
        let ceiling_lower = u64::MAX.checked_sub(resolution_mg).unwrap_or_else(|| {
            unreachable!("represented mass range contains one sampling resolution")
        });
        return lower_mg == ceiling_lower;
    }
    lower_mg.is_multiple_of(resolution_mg) && upper_mg.checked_sub(lower_mg) == Some(resolution_mg)
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
