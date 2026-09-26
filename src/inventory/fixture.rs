//! Controlled inventory-fixture admission shared by tests and external gameplay setup.
//!
//! This module does not own alternate inventory behavior. It allocates empty bootstrap storage,
//! constructs explicit starting matter, and delegates material admission and load updates to the same
//! canonical owner paths used by runtime systems. The gameplay harness reaches it only through
//! `content::gameplay_fixture`.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Mass, Temperature};
use crate::core::state::AppState;
use crate::material::{CommodityKey, MaterialComposition, MaterialLotSpec, MaterialLotSpecError};
use crate::registry::Registries;
use crate::structural::StructuralCommitError;

use super::allocation::{EmptyStockpileAllocationError, validate_empty_stockpile_allocation};
use super::ingress::{
    MaterialIngressEntry, MaterialIngressError, apply_material_ingress, validate_material_ingress,
};
use super::state::{MaterialLotId, StockpileId, StockpileStorageProfile};
use super::structural_integration::{
    StockpileStoredMassChange, StockpileStructuralLoadError, validate_stockpile_stored_mass_changes,
};

/// Failure while constructing or committing controlled starting matter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MaterialFixtureError {
    Specification(MaterialLotSpecError),
    Ingress(MaterialIngressError),
    StructuralLoad(StockpileStructuralLoadError),
    StructuralCommit(StructuralCommitError),
}

pub(crate) type AddStockpileError = EmptyStockpileAllocationError;

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

/// Adds empty material storage for tests and controlled gameplay bootstrap only.
pub(crate) fn add_stockpile(
    state: &mut AppState,
    capacity: Mass,
    storage_profile: StockpileStorageProfile,
) -> Result<StockpileId, AddStockpileError> {
    validate_empty_stockpile_allocation(state.inventory(), capacity, storage_profile)?
        .commit(state)
        .map_err(|error| {
            unreachable!("synchronous fixture stockpile allocation became stale: {error}")
        })
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

pub(crate) fn deposit_lot_for_fixture(
    registries: &Registries,
    state: &mut AppState,
    stockpile: StockpileId,
    commodity: CommodityKey,
    mass: Mass,
    temperature: Temperature,
) -> Result<MaterialLotId, MaterialFixtureError> {
    deposit_composed_lot_for_fixture(
        registries,
        state,
        stockpile,
        commodity,
        mass,
        temperature,
        MaterialComposition::pure(commodity.material()),
    )
}

pub(crate) fn deposit_composed_lot_for_fixture(
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
    deposit_lot_spec_for_fixture(registries, state, stockpile, specification)
}

/// Seeds one validated lot specification through canonical ingress and structural-load ownership.
pub(crate) fn deposit_lot_spec_for_fixture(
    registries: &Registries,
    state: &mut AppState,
    stockpile: StockpileId,
    specification: MaterialLotSpec,
) -> Result<MaterialLotId, MaterialFixtureError> {
    let created_at = state.tick();
    let entry = MaterialIngressEntry::from_lot_spec(specification, created_at);
    let ingress = validate_material_ingress(
        registries,
        state.inventory(),
        stockpile,
        [entry],
        created_at,
    )
    .map_err(MaterialFixtureError::Ingress)?;
    if !state.has_material_lot_id_headroom_from(ingress.next_lot_id(), 0) {
        return Err(MaterialFixtureError::Ingress(
            MaterialIngressError::LotIdExhausted,
        ));
    }
    let stored_after = ingress.destination_stored_mass_after(state.inventory());
    let structural = validate_stockpile_stored_mass_changes(
        registries,
        state,
        [StockpileStoredMassChange::new(stockpile, stored_after)],
    )
    .map_err(MaterialFixtureError::StructuralLoad)?;
    ingress.assert_matches_state(state.inventory());
    if let Some(structural) = structural {
        structural
            .commit(state)
            .map_err(MaterialFixtureError::StructuralCommit)?;
    }

    let resulting_lots = apply_material_ingress(state.inventory_state_mut(), ingress);
    let [resulting_lot] = resulting_lots.as_slice() else {
        unreachable!("single-parcel fixture ingress must produce exactly one resulting lot")
    };
    Ok(*resulting_lot)
}
