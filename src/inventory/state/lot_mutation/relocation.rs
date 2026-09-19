//! Feature-gated lot custody mutation used by controlled relocation transactions.

use crate::core::quantity::Mass;
use crate::core::time::SimulationTick;
use crate::inventory::coalescing::LotMergePolicy;

use super::{
    InventoryState, MaterialLotId, MaterialLotRecord, StockpileId, apply_insert_lot_record,
    apply_merge_lot_record, find_mergeable_lot,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::inventory) struct LotStorageTransition {
    at: SimulationTick,
    source_preservation_multiplier_ppm: u32,
    destination_preservation_multiplier_ppm: u32,
}

impl LotStorageTransition {
    #[must_use]
    pub(in crate::inventory) const fn new(
        at: SimulationTick,
        source_preservation_multiplier_ppm: u32,
        destination_preservation_multiplier_ppm: u32,
    ) -> Self {
        Self {
            at,
            source_preservation_multiplier_ppm,
            destination_preservation_multiplier_ppm,
        }
    }
}

pub(in crate::inventory) fn apply_move_full_lot(
    state: &mut InventoryState,
    lot: MaterialLotId,
    source: StockpileId,
    destination: StockpileId,
    storage: LotStorageTransition,
    merge_policy: LotMergePolicy,
) {
    let record = state.lots.get(&lot).unwrap_or_else(|| {
        panic!(
            "validated transfer references missing material lot {}",
            lot.value()
        )
    });
    assert_eq!(
        record.stockpile, source,
        "validated lot owner changed before commit"
    );
    let commodity = record.commodity();
    let transitioned_storage_history = record
        .storage_history
        .transition_preservation(
            storage.at,
            storage.source_preservation_multiplier_ppm,
            storage.destination_preservation_multiplier_ppm,
        )
        .unwrap_or_else(|| panic!("validated full-lot transfer has invalid storage history"));
    state.remove_lot_index(source, commodity, lot);
    let mut record = match state.lots.remove(&lot) {
        Some(record) => record,
        None => panic!(
            "validated transfer references missing material lot {}",
            lot.value()
        ),
    };
    record.storage_history = transitioned_storage_history;
    record.stockpile = destination;
    if let Some(existing_id) = find_mergeable_lot(
        state,
        destination,
        &record.profile,
        record.storage_history,
        storage.at,
        storage.destination_preservation_multiplier_ppm,
        merge_policy,
    ) {
        apply_merge_lot_record(
            state,
            existing_id,
            record,
            storage.at,
            storage.destination_preservation_multiplier_ppm,
            merge_policy,
        );
    } else {
        apply_insert_lot_record(state, record);
    }
}

pub(in crate::inventory) fn apply_split_lot(
    state: &mut InventoryState,
    source_lot: MaterialLotId,
    new_lot: MaterialLotId,
    destination: StockpileId,
    transferred: Mass,
    storage: LotStorageTransition,
    merge_policy: LotMergePolicy,
) {
    let (source_mass, source_profile, source_provenance, source_storage_history) =
        match state.lots.get(&source_lot) {
            Some(lot) => (
                lot.mass,
                lot.profile.clone(),
                lot.provenance,
                lot.storage_history,
            ),
            None => panic!(
                "validated partial transfer references missing material lot {}",
                source_lot.value()
            ),
        };
    assert!(
        transferred < source_mass,
        "partial transfer must leave positive mass in its source lot"
    );

    let source_record = match state.lots.get_mut(&source_lot) {
        Some(lot) => lot,
        None => panic!("validated partial transfer source disappeared"),
    };
    source_record.mass = match source_record.mass.checked_sub(transferred) {
        Some(value) => value,
        None => panic!("validated partial transfer underflowed source lot mass"),
    };

    let split = MaterialLotRecord {
        id: new_lot,
        stockpile: destination,
        mass: transferred,
        profile: source_profile,
        provenance: source_provenance,
        storage_history: source_storage_history
            .transition_preservation(
                storage.at,
                storage.source_preservation_multiplier_ppm,
                storage.destination_preservation_multiplier_ppm,
            )
            .unwrap_or_else(|| panic!("validated split-lot transfer has invalid storage history")),
    };
    if let Some(existing_id) = find_mergeable_lot(
        state,
        destination,
        &split.profile,
        split.storage_history,
        storage.at,
        storage.destination_preservation_multiplier_ppm,
        merge_policy,
    ) {
        apply_merge_lot_record(
            state,
            existing_id,
            split,
            storage.at,
            storage.destination_preservation_multiplier_ppm,
            merge_policy,
        );
    } else {
        apply_insert_lot_record(state, split);
    }
}
