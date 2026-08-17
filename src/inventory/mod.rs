//! Fungible matter storage with passive state, deterministic selection, and validated mutation pipelines.

#[cfg(any(test, feature = "test-gameplay"))]
mod fixture;
mod inbound_reservation;
mod ingress;
mod reserved_ingress;
mod selection;
mod state;
mod storage_validation;
mod structural_integration;
#[cfg(test)]
mod test_support;
mod transactions;

#[cfg(feature = "test-gameplay")]
pub(crate) use fixture::{deposit_composed_lot_for_fixture, deposit_lot_for_fixture};
pub(crate) use inbound_reservation::{
    InboundReservationError, ValidatedInboundReservation, validate_inbound_reservation,
};
pub(crate) use ingress::{
    MaterialIngressEntry, MaterialIngressError, ValidatedMaterialIngress, apply_material_ingress,
    validate_material_ingress,
};
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
    AddStockpileError, TransferCommitError, TransferError, ValidatedTransferBulk, add_stockpile,
    validate_transfer_bulk,
};

pub(crate) use reserved_ingress::{
    ReservedDepositPlan, ReservedDepositPlanError, ReservedDepositRequest, apply_reserved_deposits,
    decide_reserved_deposits,
};
pub(crate) use selection::{
    ConsumptionReservation, ConsumptionSelection, ConsumptionSelectionError,
    ExplicitConsumptionSelectionError, ReservationCommitError, ReservationError,
    apply_consumption_reservation, validate_consumption_reservation_from_selection,
    validate_consumption_selection, validate_explicit_consumption_selection,
};
pub(crate) use state::validate_loaded_inventory;
pub(crate) use storage_validation::validate_stockpile_storage;
pub(crate) use structural_integration::{
    StockpileStoredMassChange, ValidatedStockpileStructuralLoad, resolve_stockpile_stored_loads,
    validate_stockpile_stored_mass_changes, validate_stockpile_support_for_new_inbound,
};
pub(crate) use transactions::{
    MaterialEgressError, MaterialRelocationCommitError, MaterialRelocationError,
    ValidatedMaterialEgress, ValidatedMaterialRelocation, apply_material_egress,
    validate_material_egress_from_selection, validate_material_relocation_from_selection,
};

#[cfg(test)]
pub(crate) use test_support::{
    MaterialFixtureError, add_solid_stockpile_for_test, deposit_bulk_for_test,
    deposit_composed_lot_for_test, deposit_lot_for_test, deposit_lot_spec_for_test,
};
