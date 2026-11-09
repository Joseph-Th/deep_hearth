//! Storage-history coalescing policy contracts.

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

#[test]
fn shared_merge_history_selects_oldest_exposure_independent_of_lot_order() {
    let at = SimulationTick::new(3);
    let younger = MaterialStorageHistory::new(SimulationTick::new(2));
    let older = MaterialStorageHistory::new(SimulationTick::ZERO);
    let preservation = 1_000_000;
    let expected = (older, 3_000_000);

    assert_eq!(
        merge_compatible_storage_histories(
            younger,
            older,
            at,
            preservation,
            LotMergePolicy::OldestStorageExposure,
        ),
        expected
    );
    assert_eq!(
        merge_compatible_storage_histories(
            older,
            younger,
            at,
            preservation,
            LotMergePolicy::OldestStorageExposure,
        ),
        expected
    );
    assert_eq!(
        merge_compatible_storage_histories(
            younger,
            younger,
            at,
            preservation,
            LotMergePolicy::ExactStorageExposure,
        ),
        (younger, 1_000_000)
    );
}

#[test]
#[should_panic]
fn shared_merge_history_rejects_different_perishable_exposures() {
    merge_compatible_storage_histories(
        MaterialStorageHistory::new(SimulationTick::ZERO),
        MaterialStorageHistory::new(SimulationTick::new(1)),
        SimulationTick::new(2),
        1_000_000,
        LotMergePolicy::ExactStorageExposure,
    );
}
