//! Transient lot-coalescing policy derived from authored gameplay semantics.

use crate::material::CommodityKey;
use crate::registry::Registries;

use super::state::{MaterialLotProfile, MaterialStorageHistory};
use crate::core::time::SimulationTick;

/// Determines how compatible lot storage histories may be compacted.
///
/// Storage exposure is persisted for every material lot, but materials with any authored edible
/// form require exact exposure cohorts even while currently stored in another form. This preserves
/// the history needed by a later same-material reform into food. Other materials keep the
/// conservative oldest-exposure merge used to bound long-horizon lot fragmentation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::inventory) enum LotMergePolicy {
    OldestStorageExposure,
    ExactStorageExposure,
}

impl LotMergePolicy {
    #[must_use]
    pub(in crate::inventory) fn for_commodity(
        registries: &Registries,
        commodity: CommodityKey,
    ) -> Self {
        if registries
            .survival()
            .has_food_material(commodity.material())
        {
            Self::ExactStorageExposure
        } else {
            Self::OldestStorageExposure
        }
    }
}

pub(in crate::inventory) fn lots_are_merge_compatible(
    existing_profile: &MaterialLotProfile,
    existing_storage_history: MaterialStorageHistory,
    incoming_profile: &MaterialLotProfile,
    incoming_storage_history: MaterialStorageHistory,
    at: SimulationTick,
    preservation_multiplier_ppm: u32,
    merge_policy: LotMergePolicy,
) -> bool {
    if existing_profile != incoming_profile {
        return false;
    }
    match merge_policy {
        LotMergePolicy::OldestStorageExposure => true,
        LotMergePolicy::ExactStorageExposure => existing_storage_history
            .is_projection_equivalent(incoming_storage_history, at, preservation_multiplier_ppm)
            .unwrap_or_else(|| panic!("validated lot has invalid storage history")),
    }
}

#[cfg(test)]
#[path = "coalescing_tests.rs"]
mod tests;
