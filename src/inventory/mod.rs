//! Fungible matter storage subsystem; `state` owns records and indexes while `transactions` owns canonical mutations.

mod state;
mod transactions;

pub use state::{
    ConsumedMaterialTrace, InventoryState, InventoryValidationError, MaterialLotId,
    MaterialLotRecord, StockpileId, StockpileRecord,
};
pub use transactions::{
    AddStockpileError, DepositError, TransferCommitError, TransferError, ValidatedTransferBulk,
    add_stockpile, validate_transfer_bulk,
};

pub(crate) use state::validate_loaded_inventory;
pub(crate) use transactions::{
    ConsumptionReservation, ReservationCommitError, ReservationError,
    apply_consumption_reservation, apply_lot_cursor_and_revision, apply_reserved_deposit,
    next_material_lot_id, validate_consumption_reservation,
};

#[cfg(test)]
pub(crate) use transactions::{deposit_bulk_for_test, deposit_composed_lot_for_test};
