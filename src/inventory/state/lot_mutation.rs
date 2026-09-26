//! Child mutation layer of `InventoryState`, used only after transaction validation.

use crate::core::quantity::Mass;
use crate::core::time::SimulationTick;
use crate::material::CommodityKey;

use crate::inventory::coalescing::{LotMergePolicy, lots_are_merge_compatible};

use super::{
    InventoryState, MaterialLotId, MaterialLotProfile, MaterialLotRecord, MaterialStorageHistory,
    StockpileId, StockpileRecord,
};

mod relocation;

pub(in crate::inventory) use relocation::{
    LotStorageTransition, apply_move_full_lot, apply_split_lot,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::inventory) struct LotSlice {
    pub(in crate::inventory) lot: MaterialLotId,
    pub(in crate::inventory) mass: Mass,
}

pub(in crate::inventory) fn apply_aggregate_deposit(
    state: &mut InventoryState,
    stockpile: StockpileId,
    commodity: CommodityKey,
    mass: Mass,
) {
    let record = get_stockpile_mut_or_panic(state, stockpile);
    let current = record.get_mass(commodity);
    let next = match current.checked_add(mass) {
        Some(value) => value,
        None => panic!(
            "validated commodity mass overflow in stockpile {}",
            stockpile.value()
        ),
    };
    record.contents.insert(commodity, next);
    record.stored_mass = match record.stored_mass.checked_add(mass) {
        Some(value) => value,
        None => panic!(
            "validated stored mass overflow in stockpile {}",
            stockpile.value()
        ),
    };
}

pub(in crate::inventory) fn apply_aggregate_withdraw(
    state: &mut InventoryState,
    stockpile: StockpileId,
    commodity: CommodityKey,
    mass: Mass,
) {
    let record = get_stockpile_mut_or_panic(state, stockpile);
    let current = record.get_mass(commodity);
    let remaining = match current.checked_sub(mass) {
        Some(value) => value,
        None => panic!(
            "validated commodity mass underflow in stockpile {}",
            stockpile.value()
        ),
    };
    if remaining.is_zero() {
        record.contents.remove(&commodity);
    } else {
        record.contents.insert(commodity, remaining);
    }
    record.stored_mass = match record.stored_mass.checked_sub(mass) {
        Some(value) => value,
        None => panic!(
            "validated stored mass underflow in stockpile {}",
            stockpile.value()
        ),
    };
}

pub(in crate::inventory) fn apply_insert_or_merge_new_lot(
    state: &mut InventoryState,
    lot: MaterialLotRecord,
    merge_policy: LotMergePolicy,
    at: SimulationTick,
    destination_preservation_multiplier_ppm: u32,
) -> MaterialLotId {
    let compatible = find_mergeable_lot(
        state,
        lot.stockpile,
        &lot.profile,
        lot.storage_history,
        at,
        destination_preservation_multiplier_ppm,
        merge_policy,
    );

    let Some(existing_id) = compatible else {
        let id = lot.id;
        apply_insert_lot(state, lot);
        return id;
    };

    apply_aggregate_deposit(state, lot.stockpile, lot.commodity(), lot.mass);
    apply_merge_lot_record(
        state,
        existing_id,
        lot,
        at,
        destination_preservation_multiplier_ppm,
        merge_policy,
    );
    existing_id
}

pub(in crate::inventory) fn apply_consume_lot_slice(state: &mut InventoryState, slice: LotSlice) {
    let (source_stockpile, source_mass, commodity) = match state.lots.get(&slice.lot) {
        Some(lot) => (lot.stockpile, lot.mass, lot.commodity()),
        None => panic!(
            "validated consumption references missing material lot {}",
            slice.lot.value()
        ),
    };
    if slice.mass == source_mass {
        state.remove_lot_index(source_stockpile, commodity, slice.lot);
        let removed = state.lots.remove(&slice.lot);
        assert!(
            removed.is_some(),
            "consumed full lot must exist in lot owner"
        );
    } else {
        let lot = match state.lots.get_mut(&slice.lot) {
            Some(lot) => lot,
            None => panic!("validated partial consumption source disappeared"),
        };
        lot.mass = match lot.mass.checked_sub(slice.mass) {
            Some(value) if !value.is_zero() => value,
            Some(_) => panic!("partial consumption unexpectedly reduced lot to zero"),
            None => panic!("validated partial consumption underflowed lot mass"),
        };
    }
}

pub(in crate::inventory) fn get_stockpile_mut_or_panic(
    state: &mut InventoryState,
    stockpile: StockpileId,
) -> &mut StockpileRecord {
    match state.stockpiles.get_mut(&stockpile) {
        Some(record) => record,
        None => panic!(
            "runtime invariant broken: missing stockpile {}",
            stockpile.value()
        ),
    }
}

fn apply_insert_lot(state: &mut InventoryState, lot: MaterialLotRecord) {
    apply_aggregate_deposit(state, lot.stockpile, lot.commodity(), lot.mass);
    apply_insert_lot_record(state, lot);
}

fn apply_insert_lot_record(state: &mut InventoryState, lot: MaterialLotRecord) {
    let id = lot.id;
    let stockpile = lot.stockpile;
    let commodity = lot.commodity();
    assert!(
        !state.lots.contains_key(&id),
        "validated material lot ID must be globally unique"
    );
    state.insert_lot_index(stockpile, commodity, id);
    let replaced = state.lots.insert(id, lot);
    assert!(
        replaced.is_none(),
        "prechecked material lot ID was replaced"
    );
}

fn find_mergeable_lot(
    state: &InventoryState,
    stockpile: StockpileId,
    profile: &MaterialLotProfile,
    storage_history: MaterialStorageHistory,
    at: SimulationTick,
    preservation_multiplier_ppm: u32,
    merge_policy: LotMergePolicy,
) -> Option<MaterialLotId> {
    state
        .lot_ids_for_commodity(stockpile, profile.commodity())
        .find(|id| {
            state.lots.get(id).is_some_and(|existing| {
                lots_are_merge_compatible(
                    &existing.profile,
                    existing.storage_history,
                    profile,
                    storage_history,
                    at,
                    preservation_multiplier_ppm,
                    merge_policy,
                )
            })
        })
}

fn apply_merge_lot_record(
    state: &mut InventoryState,
    existing_id: MaterialLotId,
    lot: MaterialLotRecord,
    at: SimulationTick,
    destination_preservation_multiplier_ppm: u32,
    merge_policy: LotMergePolicy,
) {
    let existing = match state.lots.get_mut(&existing_id) {
        Some(existing) => existing,
        None => panic!(
            "runtime invariant broken: compatible lot {} disappeared during merge",
            existing_id.value()
        ),
    };
    existing.mass = match existing.mass.checked_add(lot.mass) {
        Some(value) => value,
        None => panic!("validated compatible lot merge overflowed authoritative mass"),
    };
    existing.provenance = existing.provenance.merged(lot.provenance);
    let existing_age = existing
        .storage_history
        .project(at, destination_preservation_multiplier_ppm)
        .unwrap_or_else(|| panic!("validated destination lot has invalid storage history"));
    let incoming_age = lot
        .storage_history
        .project(at, destination_preservation_multiplier_ppm)
        .unwrap_or_else(|| panic!("validated incoming lot has invalid storage history"));
    let merged_history = match merge_policy {
        LotMergePolicy::OldestStorageExposure if incoming_age > existing_age => lot.storage_history,
        LotMergePolicy::OldestStorageExposure => existing.storage_history,
        LotMergePolicy::ExactStorageExposure => {
            assert!(
                existing.storage_history.is_projection_equivalent(
                    lot.storage_history,
                    at,
                    destination_preservation_multiplier_ppm,
                ) == Some(true),
                "age-sensitive material lots with divergent storage projections must remain distinct"
            );
            existing.storage_history
        }
    };
    existing.storage_history = merged_history;
}
