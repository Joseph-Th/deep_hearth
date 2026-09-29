//! Actor-safe excavation-hardness projection for physical geological sampling.

use crate::core::quantity::Pressure;
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::material::MaterialId;
use crate::spatial::VoxelBounds;

use super::super::state::GeologicalDepositRecord;
use super::super::{ExcavationHardnessEstimate, GeologicalDepositLifecycle, GeologyState};

fn resolve_region_excavation_hardness_from_deposits<'a>(
    deposits: impl Iterator<Item = &'a GeologicalDepositRecord>,
    region: VoxelBounds,
    material: MaterialId,
    resolution: Pressure,
) -> Option<ExcavationHardnessEstimate> {
    debug_assert!(!resolution.is_zero());
    let mut minimum = None::<Pressure>;
    let mut maximum = None::<Pressure>;
    for deposit in deposits.filter(|deposit| {
        deposit.bounds().has_intersection(region)
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
    let estimate = match ExcavationHardnessEstimate::new(
        Pressure::from_pascals(lower_pa),
        Pressure::from_pascals(upper_pa),
    ) {
        Ok(estimate) => estimate,
        Err(_) => return None,
    };
    Some(estimate)
}

pub(super) fn resolve_region_excavation_hardness(
    state: &AppState,
    region: VoxelBounds,
    material: MaterialId,
    resolution: Pressure,
) -> Option<ExcavationHardnessEstimate> {
    // Live-truth projection matching abundance: depleted bodies hold no extractable matter and are
    // excluded, so a vanishing hardness band after the player's own extraction is intended feedback.
    resolve_region_excavation_hardness_from_deposits(
        state
            .geology()
            .deposits()
            .filter(|deposit| deposit.lifecycle() == GeologicalDepositLifecycle::Available),
        region,
        material,
        resolution,
    )
}

/// Replays the canonical hardness projection over bodies that existed when evidence was acquired.
pub(in crate::geology) fn resolve_historical_region_excavation_hardness(
    geology: &GeologyState,
    observed_at: SimulationTick,
    region: VoxelBounds,
    material: MaterialId,
    resolution: Pressure,
) -> Option<ExcavationHardnessEstimate> {
    resolve_region_excavation_hardness_from_deposits(
        geology
            .deposits()
            .filter(|deposit| deposit.was_available_at(observed_at)),
        region,
        material,
        resolution,
    )
}

#[cfg(test)]
#[path = "hardness_tests.rs"]
mod tests;
