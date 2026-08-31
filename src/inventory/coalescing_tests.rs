//! Storage-history coalescing policy regressions.

use super::*;

use crate::content::{FORM_LOG, MATERIAL_BERRIES, MATERIAL_WOOD, build_registries};

#[test]
fn food_material_keeps_exact_storage_history_even_in_another_form() {
    let registries = build_registries();

    assert_eq!(
        LotMergePolicy::for_commodity(&registries, CommodityKey::new(MATERIAL_BERRIES, FORM_LOG),),
        LotMergePolicy::ExactStorageExposure
    );
    assert_eq!(
        LotMergePolicy::for_commodity(&registries, CommodityKey::new(MATERIAL_WOOD, FORM_LOG)),
        LotMergePolicy::OldestStorageExposure
    );
}
