//! Actor-safe resource-scale projection for fully localized physical geological sampling.

use crate::core::quantity::Mass;
#[cfg(test)]
use crate::core::state::AppState;
use crate::material::MaterialId;
use crate::spatial::VoxelBounds;

#[cfg(test)]
use super::super::GeologicalDepositLifecycle;
use super::super::ResourceMassEstimate;
use super::super::state::GeologicalDepositRecord;

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

/// The same unique, exact-footprint body determines whether live or historical
/// evidence can include a resource-scale measurement.
pub(in crate::geology) fn localized_resource_body<'a>(
    deposits: impl Iterator<Item = &'a GeologicalDepositRecord>,
    region: VoxelBounds,
    material: MaterialId,
) -> Option<&'a GeologicalDepositRecord> {
    let mut matching = deposits.filter(|deposit| {
        deposit.bounds().has_intersection(region)
            && deposit.composition().parts_per_million(material) > 0
    });
    let deposit = matching.next()?;
    if matching.next().is_some() || region != deposit.bounds() {
        return None;
    }
    Some(deposit)
}

/// Greatest positive mass in the possible historical interval that emits the
/// exact canonical bucket. Ordinary upper endpoints are exclusive; the last
/// representable bucket can include u64::MAX. Quantize the candidate rather
/// than assuming every mass inside a widened ceiling band can emit it.
pub(in crate::geology) fn maximum_historical_resource_mass(
    resource_mass: ResourceMassEstimate,
    resolution: Mass,
    remaining: Mass,
    maximum: Mass,
) -> Option<Mass> {
    let minimum = remaining.max(Mass::from_milligrams(1));
    let upper = resource_mass.upper();
    let candidate = maximum.min(upper);
    if candidate >= minimum && resource_mass_bucket(candidate, resolution) == resource_mass {
        return Some(candidate);
    }
    // A normal bucket's upper endpoint emits the next bucket instead.
    if candidate != upper {
        return None;
    }
    let candidate = upper.checked_sub(Mass::from_milligrams(1))?;
    (candidate >= minimum && resource_mass_bucket(candidate, resolution) == resource_mass)
        .then_some(candidate)
}

pub(super) fn resolve_region_resource_mass_from_deposits<'a>(
    deposits: impl Iterator<Item = &'a GeologicalDepositRecord>,
    region: VoxelBounds,
    material: MaterialId,
    resolution: Mass,
) -> Option<ResourceMassEstimate> {
    let deposit = localized_resource_body(deposits, region, material)?;
    Some(resource_mass_bucket(deposit.remaining_mass(), resolution))
}

#[cfg(test)]
pub(crate) fn resolve_region_resource_mass(
    state: &AppState,
    region: VoxelBounds,
    material: MaterialId,
    resolution: Mass,
) -> Option<ResourceMassEstimate> {
    resolve_region_resource_mass_from_deposits(
        state
            .geology()
            .deposits_intersecting(region)
            .filter(|deposit| deposit.lifecycle() == GeologicalDepositLifecycle::Available),
        region,
        material,
        resolution,
    )
}

#[cfg(test)]
#[path = "resource_mass_tests.rs"]
mod tests;
