//! Low-level inventory lot and aggregate mutations used only after transaction validation.

use crate::core::quantity::Mass;
use crate::material::CommodityKey;

use super::state::{
    InventoryState, MaterialLotId, MaterialLotProfile, MaterialLotRecord, StockpileId,
    StockpileRecord,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct LotSlice {
    pub(super) lot: MaterialLotId,
    pub(super) mass: Mass,
}

pub(super) fn apply_aggregate_deposit(
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

pub(super) fn apply_aggregate_withdraw(
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

pub(super) fn apply_insert_or_merge_new_lot(
    state: &mut InventoryState,
    lot: MaterialLotRecord,
) -> MaterialLotId {
    let compatible = find_compatible_lot(state, lot.stockpile, &lot.profile);

    let Some(existing_id) = compatible else {
        let id = lot.id;
        apply_insert_lot(state, lot);
        return id;
    };

    apply_aggregate_deposit(state, lot.stockpile, lot.commodity(), lot.mass);
    apply_merge_lot_record(state, existing_id, lot);
    existing_id
}

pub(super) fn apply_move_full_lot(
    state: &mut InventoryState,
    lot: MaterialLotId,
    source: StockpileId,
    destination: StockpileId,
) {
    let removed = get_stockpile_mut_or_panic(state, source)
        .lot_ids
        .remove(&lot);
    assert!(
        removed,
        "validated source stockpile must index moved material lot"
    );
    let inserted = get_stockpile_mut_or_panic(state, destination)
        .lot_ids
        .insert(lot);
    assert!(
        inserted,
        "destination stockpile must not already index moved material lot"
    );
    let record = match state.lots.get_mut(&lot) {
        Some(record) => record,
        None => panic!(
            "validated transfer references missing material lot {}",
            lot.value()
        ),
    };
    assert_eq!(
        record.stockpile, source,
        "validated lot owner changed before commit"
    );
    record.stockpile = destination;
}

pub(super) fn apply_split_lot(
    state: &mut InventoryState,
    source_lot: MaterialLotId,
    new_lot: MaterialLotId,
    destination: StockpileId,
    transferred: Mass,
) {
    let source_snapshot = match state.lots.get(&source_lot) {
        Some(lot) => lot.clone(),
        None => panic!(
            "validated partial transfer references missing material lot {}",
            source_lot.value()
        ),
    };
    assert!(
        transferred < source_snapshot.mass,
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
        profile: source_snapshot.profile.clone(),
        provenance: source_snapshot.provenance,
    };
    if let Some(existing_id) = find_compatible_lot(state, destination, &split.profile) {
        apply_merge_lot_record(state, existing_id, split);
    } else {
        apply_insert_lot_record(state, split);
    }
}

pub(super) fn apply_consume_lot_slice(state: &mut InventoryState, slice: LotSlice) {
    let snapshot = match state.lots.get(&slice.lot) {
        Some(lot) => lot.clone(),
        None => panic!(
            "validated consumption references missing material lot {}",
            slice.lot.value()
        ),
    };
    if slice.mass == snapshot.mass {
        let removed = get_stockpile_mut_or_panic(state, snapshot.stockpile)
            .lot_ids
            .remove(&slice.lot);
        assert!(removed, "consumed full lot must exist in owner index");
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

pub(super) fn get_stockpile_mut_or_panic(
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

pub(super) fn apply_insert_lot(state: &mut InventoryState, lot: MaterialLotRecord) {
    apply_aggregate_deposit(state, lot.stockpile, lot.commodity(), lot.mass);
    apply_insert_lot_record(state, lot);
}

fn apply_insert_lot_record(state: &mut InventoryState, lot: MaterialLotRecord) {
    let id = lot.id;
    let stockpile = lot.stockpile;
    let inserted = get_stockpile_mut_or_panic(state, stockpile)
        .lot_ids
        .insert(id);
    assert!(
        inserted,
        "validated material lot ID must be unique in owner index"
    );
    let replaced = state.lots.insert(id, lot);
    assert!(
        replaced.is_none(),
        "validated material lot ID must be globally unique"
    );
}

fn find_compatible_lot(
    state: &InventoryState,
    stockpile: StockpileId,
    profile: &MaterialLotProfile,
) -> Option<MaterialLotId> {
    let owner = match state.stockpiles.get(&stockpile) {
        Some(owner) => owner,
        None => panic!(
            "runtime invariant broken: missing destination stockpile {}",
            stockpile.value()
        ),
    };
    owner.lot_ids.iter().copied().find(|id| {
        state
            .lots
            .get(id)
            .is_some_and(|existing| &existing.profile == profile)
    })
}

fn apply_merge_lot_record(
    state: &mut InventoryState,
    existing_id: MaterialLotId,
    lot: MaterialLotRecord,
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
    existing.provenance.earliest_created_at = std::cmp::min(
        existing.provenance.earliest_created_at,
        lot.provenance.earliest_created_at,
    );
    existing.provenance.latest_created_at = std::cmp::max(
        existing.provenance.latest_created_at,
        lot.provenance.latest_created_at,
    );
}
