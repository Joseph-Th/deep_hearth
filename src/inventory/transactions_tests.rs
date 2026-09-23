//! Contract tests for inventory transaction atomicity.

use std::collections::BTreeMap;

use super::*;
use crate::content::{
    FORM_CHIP, FORM_FOOD, FORM_INGOT, FORM_LOG, FORM_LUMP, FORM_MOLTEN, FORM_ORE, MATERIAL_BERRIES,
    MATERIAL_COPPER, MATERIAL_SLAG, MATERIAL_STONE, MATERIAL_WOOD, build_registries,
};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::{AppState, apply_clock_advance, validate_loaded_state};
use crate::core::time::SimulationTick;
use crate::energy::calculate_explicit_energy_accounting;
use crate::inventory::selection::apply_consumption_reservation;
use crate::inventory::{
    MaterialFixtureError, MaterialIngressEntry, MaterialIngressError, MaterialLotId,
    MaterialLotRecord, ReservedDepositRequest, StockpileId, StockpileStorageError,
    StockpileStorageProfile, add_solid_stockpile_for_test, add_stockpile, apply_material_ingress,
    apply_reserved_deposits, decide_reserved_deposits, deposit_bulk_for_test,
    deposit_composed_lot_for_test, deposit_lot_for_test,
    validate_consumption_reservation_from_selection, validate_consumption_selection,
    validate_loaded_inventory, validate_material_ingress, validate_material_transfer_for_test,
};
use crate::material::{
    CommodityKey, CompositionComponent, MaterialComposition, MaterialInputSpec, MaterialLotSpec,
    MaterialPhase,
};
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::registry::Registries;

fn wood_log() -> CommodityKey {
    CommodityKey::new(MATERIAL_WOOD, FORM_LOG)
}

fn projected_storage_age_parts(state: &AppState, lot: MaterialLotId) -> u128 {
    let record = state
        .inventory()
        .get_lot(lot)
        .unwrap_or_else(|| panic!("storage-age fixture lot {} disappeared", lot.value()));
    let preservation = state
        .inventory()
        .get_stockpile(record.stockpile())
        .unwrap_or_else(|| panic!("storage-age fixture stockpile disappeared"))
        .storage_profile()
        .preservation_multiplier_ppm();
    record
        .storage_history()
        .project(state.tick(), preservation)
        .unwrap_or_else(|| panic!("storage-age fixture projection overflowed"))
}

fn triple_preservation_profile() -> StockpileStorageProfile {
    StockpileStorageProfile::with_preservation(
        true,
        false,
        Temperature::from_millikelvin(350_000),
        3_000_000,
    )
    .unwrap_or_else(|error| panic!("triple-preservation fixture profile failed: {error}"))
}

fn split_transfer_fixture() -> (Registries, AppState, StockpileId, StockpileId) {
    let registries = build_registries();
    let mut state = AppState::new();
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20))
        .unwrap_or_else(|error| panic!("split-transfer source fixture failed: {error}"));
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20))
        .unwrap_or_else(|error| panic!("split-transfer destination fixture failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        wood_log(),
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(300_000),
    )
    .unwrap_or_else(|error| panic!("split-transfer material fixture failed: {error}"));
    (registries, state, source, destination)
}

#[path = "transactions_tests/transfer.rs"]
mod transfer;

fn stored_lot_total(state: &AppState) -> Mass {
    state.inventory().lots().fold(Mass::ZERO, |acc, lot| {
        acc.checked_add(lot.mass())
            .unwrap_or_else(|| panic!("conservation test overflow"))
    })
}

fn stored_aggregate_total(state: &AppState) -> Mass {
    state
        .inventory()
        .stockpiles()
        .fold(Mass::ZERO, |acc, pile| {
            acc.checked_add(pile.stored_mass())
                .unwrap_or_else(|| panic!("conservation test overflow"))
        })
}

fn assert_lot_aggregate_agreement(registries: &Registries, state: &AppState, label: &str) {
    assert_eq!(
        stored_lot_total(state),
        stored_aggregate_total(state),
        "{label}: lot total disagrees with stockpile aggregate total"
    );
    assert_eq!(
        validate_loaded_inventory(registries.materials(), state.inventory(), state.tick()),
        Ok(())
    );
}

#[path = "transactions_tests/conservation.rs"]
mod conservation;

#[path = "transactions_tests/reform.rs"]
mod reform;
