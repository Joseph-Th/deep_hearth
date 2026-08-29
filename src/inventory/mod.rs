//! Owns inventory custody, deterministic material selection, and validated matter mutation.

mod coalescing;
mod enclosure_execution;
mod enclosure_validation;
#[cfg(any(test, feature = "test-gameplay"))]
mod fixture;
mod inbound_reservation;
mod ingress;
mod lot_identity;
mod reserved_ingress;
mod selection;
mod state;
mod storage;
mod storage_validation;
mod structural_integration;
#[cfg(test)]
mod test_support;
mod transactions;

pub use enclosure_execution::{
    StorageEnclosureCommitError, StorageEnclosureConstructionError,
    ValidatedStorageEnclosureConstruction, validate_build_storage_enclosure,
};
pub use enclosure_validation::StorageEnclosureValidationError;
pub(crate) use enclosure_validation::validate_loaded_storage_enclosures;
#[cfg(any(test, feature = "test-gameplay"))]
pub(crate) use fixture::add_stockpile;
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
    MaterialLotRecord, StockpileEnclosureRecord, StockpileId, StockpileRecord,
    StockpileStorageProfile, StockpileStorageProfileError,
};
pub use storage::{StorageDefinition, StorageDefinitionId, StorageRegistry};
pub use storage_validation::StockpileStorageError;
pub use structural_integration::{
    StockpileStructuralLoadError, StockpileSupportCommitError, StockpileSupportError,
    StockpileSupportOutcome, ValidatedStockpileSupportChange, validate_mount_stockpile,
    validate_unmount_stockpile,
};
#[cfg(any(test, feature = "test-gameplay"))]
pub use transactions::{
    MaterialTransferCommitError, MaterialTransferError, MaterialTransferResolution,
    ValidatedMaterialTransfer, validate_material_transfer,
};

pub(crate) use reserved_ingress::{
    ReservedDepositPlan, ReservedDepositPlanError, ReservedDepositRequest, apply_reserved_deposits,
    decide_reserved_deposits,
};
pub(crate) use selection::{
    ConsumptionReservation, ConsumptionSelection, ConsumptionSelectionError,
    ExplicitConsumptionSelectionError, ReservationError, apply_prechecked_consumption_reservation,
    validate_consumption_reservation_from_selection, validate_consumption_selection,
    validate_explicit_consumption_selection,
};
pub(crate) use state::{
    AMBIENT_PRESERVATION_MULTIPLIER_PPM, MaterialStorageHistory, STORAGE_AGE_PARTS_PER_TICK,
    validate_loaded_inventory,
};
pub(crate) use storage_validation::validate_stockpile_storage;
pub(crate) use structural_integration::{
    StockpileStoredMassChange, ValidatedStockpileStructuralLoad,
    validate_stockpile_stored_mass_changes, validate_stockpile_support_for_new_inbound,
};
pub(crate) use transactions::{
    MaterialEgressError, MaterialReformCommitError, MaterialReformError, ValidatedMaterialEgress,
    ValidatedMaterialReform, apply_material_egress, validate_material_egress_from_selection,
    validate_material_reform_from_selection,
};

#[cfg(test)]
pub(crate) use test_support::{
    MaterialFixtureError, add_solid_stockpile_for_test, deposit_bulk_for_test,
    deposit_composed_lot_for_test, deposit_lot_for_test, deposit_lot_spec_for_test,
    validate_material_transfer_for_test,
};
