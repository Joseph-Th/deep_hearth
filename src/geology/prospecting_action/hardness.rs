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
    let lower_pa = minimum
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
        maximum_pa
            .checked_div(resolution_pa)
            .and_then(|bucket| bucket.checked_add(1))
            .and_then(|bucket| bucket.checked_mul(resolution_pa))
            .unwrap_or(u64::MAX)
    };
    Some(
        ExcavationHardnessEstimate::new(
            Pressure::from_pascals(lower_pa),
            Pressure::from_pascals(upper_pa),
        )
        .unwrap_or_else(|error| {
            unreachable!("resolved excavation-hardness bucket must be valid: {error}")
        }),
    )
}

#[cfg(test)]
#[path = "hardness_tests.rs"]
mod tests;
