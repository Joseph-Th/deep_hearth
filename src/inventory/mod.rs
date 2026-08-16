//! Fungible matter storage with passive state, deterministic selection, and validated mutation pipelines.

mod lot_mutation;
mod selection;
mod state;
mod storage_validation;
mod structural_integration;
mod transactions;

pub use selection::MaterialLotSelection;
pub use state::{
    ConsumedMaterialTrace, InventoryState, InventoryValidationError, MaterialLotId,
    MaterialLotRecord, StockpileId, StockpileRecord, StockpileStorageProfile,
    StockpileStorageProfileError,
};
pub use storage_validation::StockpileStorageError;
pub use structural_integration::{
    StockpileStructuralLoadError, StockpileSupportCommitError, StockpileSupportError,
    StockpileSupportOutcome, ValidatedStockpileSupportChange, validate_mount_stockpile,
    validate_unmount_stockpile,
};
pub use transactions::{
    AddStockpileError, DepositError, TransferCommitError, TransferError, ValidatedTransferBulk,
    add_stockpile, validate_transfer_bulk,
};

pub(crate) use selection::{
    ConsumptionReservation, ConsumptionSelection, ConsumptionSelectionError,
    ExplicitConsumptionSelectionError, ReservationCommitError, ReservationError,
    apply_consumption_reservation, apply_reserved_deposit,
    validate_consumption_reservation_from_selection, validate_consumption_selection,
    validate_explicit_consumption_selection,
};
pub(crate) use state::validate_loaded_inventory;
pub(crate) use storage_validation::validate_stockpile_storage;
pub(crate) use structural_integration::{
    StockpileStoredMassChange, ValidatedStockpileStructuralLoad, resolve_stockpile_stored_loads,
    validate_stockpile_stored_mass_changes, validate_stockpile_support_for_new_inbound,
};
pub(crate) use transactions::{
    MaterialBatchIngressError, MaterialEgressError, MaterialIngressError,
    MaterialRelocationCommitError, MaterialRelocationError, ValidatedMaterialBatchIngress,
    ValidatedMaterialEgress, ValidatedMaterialIngress, ValidatedMaterialRelocation,
    apply_lot_cursor_and_revision, apply_material_batch_ingress, apply_material_egress,
    apply_material_ingress, next_material_lot_id, validate_material_batch_ingress,
    validate_material_egress_from_selection, validate_material_ingress,
    validate_material_relocation_from_selection,
};

#[cfg(test)]
pub(crate) use transactions::{
    add_solid_stockpile_for_test, deposit_bulk_for_test, deposit_composed_lot_for_test,
    deposit_lot_for_test, deposit_lot_spec_for_test,
};
