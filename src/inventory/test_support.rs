//! Unit-test conveniences over the shared canonical inventory fixture boundary.

pub(crate) use super::fixture::MaterialFixtureError;
use super::fixture::{
    AddStockpileError, add_stockpile, deposit_composed_lot_for_fixture, deposit_lot_for_fixture,
    deposit_lot_spec_for_fixture,
};
use super::state::{MaterialLotId, StockpileId, StockpileStorageProfile};
use super::transactions::{
    MaterialTransferError, MaterialTransferResolution, ValidatedMaterialTransfer,
    validate_material_transfer,
};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::AppState;
use crate::material::{CommodityKey, MaterialComposition, MaterialLotSpec};
use crate::registry::Registries;

#[cfg(test)]
const TEST_REFERENCE_TEMPERATURE: Temperature = Temperature::from_millikelvin(293_150);

pub(crate) fn add_solid_stockpile_for_test(
    state: &mut AppState,
    capacity: Mass,
) -> Result<StockpileId, AddStockpileError> {
    add_stockpile(state, capacity, StockpileStorageProfile::solid_only())
}

/// Validates one controlled pathless transfer fixture through the canonical transfer boundary.
pub(crate) fn validate_material_transfer_for_test(
    registries: &Registries,
    state: &AppState,
    source: StockpileId,
    destination: StockpileId,
    commodity: CommodityKey,
    mass: Mass,
) -> Result<ValidatedMaterialTransfer, MaterialTransferError> {
    validate_material_transfer(
        registries,
        state,
        MaterialTransferResolution::new(source, destination, commodity, mass),
    )
}

/// Deposits explicitly sourced matter after validating references and capacity.
#[cfg(test)]
pub(crate) fn deposit_bulk_for_test(
    registries: &Registries,
    state: &mut AppState,
    stockpile: StockpileId,
    commodity: CommodityKey,
    mass: Mass,
) -> Result<(), MaterialFixtureError> {
    deposit_lot_for_test(
        registries,
        state,
        stockpile,
        commodity,
        mass,
        TEST_REFERENCE_TEMPERATURE,
    )
    .map(|_| ())
}

/// Seeds one explicit homogeneous lot for tests that need controlled thermal state.
pub(crate) fn deposit_lot_for_test(
    registries: &Registries,
    state: &mut AppState,
    stockpile: StockpileId,
    commodity: CommodityKey,
    mass: Mass,
    temperature: Temperature,
) -> Result<MaterialLotId, MaterialFixtureError> {
    deposit_lot_for_fixture(registries, state, stockpile, commodity, mass, temperature)
}

pub(crate) fn deposit_composed_lot_for_test(
    registries: &Registries,
    state: &mut AppState,
    stockpile: StockpileId,
    commodity: CommodityKey,
    mass: Mass,
    temperature: Temperature,
    composition: MaterialComposition,
) -> Result<MaterialLotId, MaterialFixtureError> {
    deposit_composed_lot_for_fixture(
        registries,
        state,
        stockpile,
        commodity,
        mass,
        temperature,
        composition,
    )
}

/// Seeds one already-validated lot specification through canonical inventory ingress.
pub(crate) fn deposit_lot_spec_for_test(
    registries: &Registries,
    state: &mut AppState,
    stockpile: StockpileId,
    specification: MaterialLotSpec,
) -> Result<MaterialLotId, MaterialFixtureError> {
    deposit_lot_spec_for_fixture(registries, state, stockpile, specification)
}
