//! Actor-safe excavation-hardness projection for physical geological sampling.

use crate::core::quantity::Pressure;
use crate::core::state::AppState;
use crate::material::MaterialId;
use crate::spatial::VoxelBounds;

use super::super::{ExcavationHardnessEstimate, GeologicalDepositLifecycle};

pub(super) fn resolve_region_excavation_hardness(
    state: &AppState,
    region: VoxelBounds,
    material: MaterialId,
    resolution: Pressure,
) -> Option<ExcavationHardnessEstimate> {
    debug_assert!(!resolution.is_zero());
    // Live-truth projection matching abundance: depleted bodies hold no extractable matter and are
    // excluded, so a vanishing hardness band after the player's own extraction is intended feedback.
    let mut minimum = None::<Pressure>;
    let mut maximum = None::<Pressure>;
    for deposit in state.geology().deposits().filter(|deposit| {
        deposit.lifecycle() == GeologicalDepositLifecycle::Available
            && deposit.bounds().has_intersection(region)
            && deposit.composition().parts_per_million(material) > 0
    }) {
        let hardness = deposit.excavation_hardness();
        minimum = Some(minimum.map_or(hardness, |current| current.min(hardness)));
        maximum = Some(maximum.map_or(hardness, |current| current.max(hardness)));
    }
    let (minimum, maximum) = (minimum?, maximum?);
    let resolution_pa = resolution.pascals();
    // Under-approximate the lower edge of the conservative hardness band.
    let mut lower_pa = minimum
        .pascals()
        .saturating_sub(1)
        .checked_div(resolution_pa)
        .unwrap_or_else(|| unreachable!("nonzero hardness resolution divides pressure"))
        .checked_mul(resolution_pa)
        .unwrap_or_else(|| unreachable!("hardness lower bucket cannot exceed its input"));
    let maximum_pa = maximum.pascals();
    let upper_pa = if maximum_pa.is_multiple_of(resolution_pa) {
        maximum_pa
    } else {
        let aligned_upper = maximum_pa
            .checked_div(resolution_pa)
            .and_then(|bucket| bucket.checked_add(1))
            .and_then(|bucket| bucket.checked_mul(resolution_pa));
        match aligned_upper {
            Some(upper_pa) => upper_pa,
            None => {
                let ceiling_lower = u64::MAX.checked_sub(resolution_pa).unwrap_or_else(|| {
                    unreachable!("represented pressure range contains one sampling resolution")
                });
                lower_pa = lower_pa.min(ceiling_lower);
                u64::MAX
            }
        }
    };
    // Zero-hardness deposits carry no measurable excavation resistance; trusted load must not panic.
    let estimate = match ExcavationHardnessEstimate::new(
        Pressure::from_pascals(lower_pa),
        Pressure::from_pascals(upper_pa),
    ) {
        Ok(estimate) => estimate,
        Err(_) => return None,
    };
    Some(estimate)
}

pub(in crate::geology) fn excavation_hardness_band_matches_resolution(
    hardness: ExcavationHardnessEstimate,
    resolution: Pressure,
) -> bool {
    let resolution_pa = resolution.pascals();
    if resolution_pa == 0 {
        return false;
    }
    let lower_pa = hardness.lower().pascals();
    let upper_pa = hardness.upper().pascals();
    if upper_pa <= lower_pa {
        return false;
    }
    if upper_pa == u64::MAX {
        let ceiling_lower = u64::MAX.checked_sub(resolution_pa).unwrap_or_else(|| {
            unreachable!("represented pressure range contains one sampling resolution")
        });
        return lower_pa == ceiling_lower
            || (lower_pa.is_multiple_of(resolution_pa) && lower_pa <= ceiling_lower);
    }
    lower_pa.is_multiple_of(resolution_pa) && upper_pa.is_multiple_of(resolution_pa)
}

#[cfg(test)]
#[path = "hardness_tests.rs"]
mod tests;
