//! Shared inventory fixtures routed through canonical ingress and structural-load transactions.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Mass, Temperature};
use crate::core::state::AppState;
use crate::material::{CommodityKey, MaterialComposition, MaterialLotSpec, MaterialLotSpecError};
use crate::registry::Registries;
use crate::structural::StructuralCommitError;

use super::state::{MaterialLotId, StockpileId, StockpileStorageProfile};
use super::structural_integration::{
    StockpileStoredMassChange, StockpileStructuralLoadError, validate_stockpile_stored_mass_changes,
};
use super::transactions::{
    AddStockpileError, MaterialIngressError, add_stockpile, apply_material_ingress,
    validate_material_ingress,
};

#[cfg(test)]
const TEST_REFERENCE_TEMPERATURE: Temperature = Temperature::from_millikelvin(293_150);

/// Failure while constructing or committing a canonical material fixture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MaterialFixtureError {
    Specification(MaterialLotSpecError),
    Ingress(MaterialIngressError),
    StructuralLoad(StockpileStructuralLoadError),
    StructuralCommit(StructuralCommitError),
}

impl Display for MaterialFixtureError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Specification(error) => write!(formatter, "invalid material fixture: {error}"),
            Self::Ingress(error) => write!(formatter, "material fixture ingress failed: {error}"),
            Self::StructuralLoad(error) => write!(
                formatter,
                "material fixture cannot update stored-matter support load: {error}"
            ),
            Self::StructuralCommit(error) => write!(
                formatter,
                "material fixture could not commit stored-matter structural load: {error}"
            ),
        }
    }
}

impl Error for MaterialFixtureError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Specification(error) => Some(error),
            Self::Ingress(error) => Some(error),
            Self::StructuralLoad(error) => Some(error),
            Self::StructuralCommit(error) => Some(error),
        }
    }
}

pub(crate) fn add_solid_stockpile_for_test(
    state: &mut AppState,
    capacity: Mass,
) -> Result<StockpileId, AddStockpileError> {
    add_stockpile(state, capacity, StockpileStorageProfile::solid_only())
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
    deposit_composed_lot_for_test(
        registries,
        state,
        stockpile,
        commodity,
        mass,
        temperature,
        MaterialComposition::pure(commodity.material()),
    )
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
    let specification =
        MaterialLotSpec::with_composition(commodity, mass, temperature, composition)
            .map_err(MaterialFixtureError::Specification)?;
    deposit_lot_spec_for_test(registries, state, stockpile, specification)
}

/// Seeds one already-validated lot specification through canonical inventory ingress.
pub(crate) fn deposit_lot_spec_for_test(
    registries: &Registries,
    state: &mut AppState,
    stockpile: StockpileId,
    specification: MaterialLotSpec,
) -> Result<MaterialLotId, MaterialFixtureError> {
    let mass = specification.mass();
    let ingress = validate_material_ingress(
        registries,
        state.inventory(),
        stockpile,
        specification,
        state.tick(),
    )
    .map_err(MaterialFixtureError::Ingress)?;
    let record = state
        .inventory()
        .get_stockpile(stockpile)
        .unwrap_or_else(|| panic!("validated test ingress destination disappeared"));
    let stored_after = record
        .stored_mass()
        .checked_add(mass)
        .unwrap_or_else(|| panic!("validated test ingress overflowed stored mass"));
    let structural = validate_stockpile_stored_mass_changes(
        registries,
        state,
        [StockpileStoredMassChange::new(stockpile, stored_after)],
    )
    .map_err(MaterialFixtureError::StructuralLoad)?;
    if let Some(structural) = structural {
        structural
            .commit(state)
            .map_err(MaterialFixtureError::StructuralCommit)?;
    }

    Ok(apply_material_ingress(state.inventory_state_mut(), ingress))
}
