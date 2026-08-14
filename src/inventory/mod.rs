//! Fungible matter storage subsystem; `state` owns records and indexes while `transactions` owns canonical mutations.

mod state;
mod structural_integration;
mod transactions;

pub use state::{
    ConsumedMaterialTrace, InventoryState, InventoryValidationError, MaterialLotId,
    MaterialLotRecord, StockpileId, StockpileRecord, StockpileStorageProfile,
    StockpileStorageProfileError,
};
pub use structural_integration::{
    StockpileStructuralLoadError, StockpileSupportCommitError, StockpileSupportError,
    StockpileSupportOutcome, ValidatedStockpileSupportChange, validate_mount_stockpile,
    validate_unmount_stockpile,
};
pub use transactions::{
    AddStockpileError, DepositError, MaterialLotSelection, StockpileStorageError,
    TransferCommitError, TransferError, ValidatedTransferBulk, add_stockpile,
    add_stockpile_with_storage_profile, validate_transfer_bulk,
};

pub(crate) use state::validate_loaded_inventory;
pub(crate) use structural_integration::{
    StockpileStoredMassChange, ValidatedStockpileStructuralLoad, resolve_stockpile_stored_loads,
    validate_stockpile_stored_mass_changes, validate_stockpile_support_for_new_inbound,
};
pub(crate) use transactions::{
    ConsumptionReservation, ConsumptionSelection, ConsumptionSelectionError,
    ExplicitConsumptionSelectionError, MaterialBatchIngressError, MaterialEgressError,
    MaterialIngressError, MaterialRelocationCommitError, MaterialRelocationError,
    ReservationCommitError, ReservationError, ValidatedMaterialBatchIngress,
    ValidatedMaterialEgress, ValidatedMaterialIngress, ValidatedMaterialRelocation,
    apply_consumption_reservation, apply_lot_cursor_and_revision, apply_material_batch_ingress,
    apply_material_egress, apply_material_ingress, apply_reserved_deposit, next_material_lot_id,
    validate_consumption_reservation_from_selection, validate_consumption_selection,
    validate_explicit_consumption_selection, validate_material_batch_ingress,
    validate_material_egress_from_selection, validate_material_ingress,
    validate_material_relocation_from_selection, validate_stockpile_storage,
};

#[cfg(test)]
pub(crate) use transactions::{
    deposit_bulk_for_test, deposit_composed_lot_for_test, deposit_lot_for_test,
    deposit_lot_spec_for_test,
};

#[cfg(test)]
mod explicit_selection_tests {
    use super::*;
    use crate::content::{FORM_LOG, MATERIAL_WOOD, build_registries};
    use crate::core::quantity::{Mass, Temperature};
    use crate::core::state::AppState;
    use crate::core::time::WorldSeed;
    use crate::material::CommodityKey;

    #[test]
    fn explicit_selection_binds_partial_lot_without_mutation() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x1A70_0001));
        let source = match add_stockpile(&mut state, Mass::from_milligrams(100)) {
            Ok(source) => source,
            Err(error) => panic!("explicit selection source fixture failed: {error}"),
        };
        let lot = match deposit_lot_for_test(
            &registries,
            &mut state,
            source,
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(20),
            Temperature::from_millikelvin(300_000),
        ) {
            Ok(lot) => lot,
            Err(error) => panic!("explicit selection lot fixture failed: {error}"),
        };
        let before = state.clone();

        let selection = match validate_explicit_consumption_selection(
            state.inventory_state(),
            source,
            &[MaterialLotSelection::new(lot, Mass::from_milligrams(7))],
        ) {
            Ok(selection) => selection,
            Err(error) => panic!("explicit selection validation failed: {error:?}"),
        };

        assert_eq!(selection.total_consumed(), Mass::from_milligrams(7));
        assert_eq!(selection.consumed_inputs().len(), 1);
        assert_eq!(
            selection.consumed_inputs()[0].mass(),
            Mass::from_milligrams(7)
        );
        assert_eq!(
            selection.consumed_inputs()[0].profile().temperature(),
            Temperature::from_millikelvin(300_000)
        );
        assert_eq!(state, before);
    }

    #[test]
    fn explicit_selection_rejects_duplicate_lot_and_wrong_source() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x1A70_0002));
        let source = match add_stockpile(&mut state, Mass::from_milligrams(100)) {
            Ok(source) => source,
            Err(error) => panic!("explicit selection source fixture failed: {error}"),
        };
        let other = match add_stockpile(&mut state, Mass::from_milligrams(100)) {
            Ok(source) => source,
            Err(error) => panic!("explicit selection secondary fixture failed: {error}"),
        };
        let lot = match deposit_lot_for_test(
            &registries,
            &mut state,
            source,
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(20),
            Temperature::from_millikelvin(300_000),
        ) {
            Ok(lot) => lot,
            Err(error) => panic!("explicit selection lot fixture failed: {error}"),
        };
        let before = state.clone();
        let slice = MaterialLotSelection::new(lot, Mass::from_milligrams(5));

        assert_eq!(
            validate_explicit_consumption_selection(
                state.inventory_state(),
                source,
                &[slice, slice],
            ),
            Err(ExplicitConsumptionSelectionError::DuplicateLot { lot })
        );
        assert_eq!(
            validate_explicit_consumption_selection(state.inventory_state(), other, &[slice]),
            Err(ExplicitConsumptionSelectionError::LotOwnedElsewhere {
                lot,
                requested_source: other,
                actual_source: source,
            })
        );
        assert_eq!(state, before);
    }
}
