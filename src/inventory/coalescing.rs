//! Transient lot-coalescing policy derived from authored gameplay semantics.

use crate::material::CommodityKey;
use crate::registry::Registries;

/// Determines how compatible lot storage histories may be compacted.
///
/// Storage exposure is persisted for every material lot, but only commodities with authored
/// age-dependent behavior require exact exposure cohorts. Other commodities keep the conservative
/// oldest-exposure merge used to bound long-horizon lot fragmentation.
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
        if registries.survival().get_food(commodity).is_some() {
            Self::ExactStorageExposure
        } else {
            Self::OldestStorageExposure
        }
    }
}
