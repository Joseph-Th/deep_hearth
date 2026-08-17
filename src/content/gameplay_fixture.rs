//! External starting-state boundary for the gameplay exercise.
//!
//! Deep Hearth does not yet own world acquisition for loose matter, stored-energy generation, or a
//! player-facing structural construction authorizer. The gameplay harness therefore has to arrange
//! those facts before the acting policy starts. Keep every direct bootstrap-only mutation in this
//! module so the exercise itself cannot accidentally treat a fixture shortcut as player behavior.
//!
//! Once setup returns, gameplay code must use the same runtime resolvers, validators, commits, and
//! simulation ticks as the game core.

use crate::core::quantity::{Energy, Mass, Temperature};
use crate::core::state::AppState;
use crate::energy::{
    EnergyStoreDefinitionId, EnergyStoreId, add_energy_store_with_initial_for_fixture,
};
use crate::inventory::{
    MaterialLotId, MaterialLotSelection, StockpileId, StockpileStorageProfile, add_stockpile,
    deposit_composed_lot_for_fixture, deposit_lot_for_fixture,
};
use crate::material::{CommodityKey, FormId, MaterialComposition};
use crate::registry::Registries;
use crate::structural::{
    StructuralElementId, bind_structural_construction_selection,
    resolve_structural_material_requirement, validate_structural_construction,
};

pub fn seed_energy_store(
    registries: &Registries,
    state: &mut AppState,
    definition: EnergyStoreDefinitionId,
    amount: Energy,
) -> EnergyStoreId {
    add_energy_store_with_initial_for_fixture(registries, state, definition, amount)
        .unwrap_or_else(|error| panic!("gameplay bootstrap energy seed failed: {error}"))
}

pub fn seed_lot(
    registries: &Registries,
    state: &mut AppState,
    stockpile: StockpileId,
    commodity: CommodityKey,
    mass: Mass,
    temperature: Temperature,
) -> MaterialLotId {
    deposit_lot_for_fixture(registries, state, stockpile, commodity, mass, temperature)
        .unwrap_or_else(|error| panic!("gameplay bootstrap material seed failed: {error}"))
}

pub fn seed_composed_lot(
    registries: &Registries,
    state: &mut AppState,
    stockpile: StockpileId,
    commodity: CommodityKey,
    mass: Mass,
    temperature: Temperature,
    composition: MaterialComposition,
) -> MaterialLotId {
    deposit_composed_lot_for_fixture(
        registries,
        state,
        stockpile,
        commodity,
        mass,
        temperature,
        composition,
    )
    .unwrap_or_else(|error| panic!("gameplay bootstrap composed-material seed failed: {error}"))
}

pub fn materialize_structure(
    registries: &Registries,
    state: &mut AppState,
    element: StructuralElementId,
    form: FormId,
) {
    let requirement = resolve_structural_material_requirement(registries, state, element)
        .unwrap_or_else(|error| panic!("gameplay bootstrap material requirement failed: {error}"));
    let mass = requirement.required_mass();
    let source =
        add_stockpile(state, mass, StockpileStorageProfile::solid_only()).unwrap_or_else(|error| {
            panic!("gameplay bootstrap construction stockpile failed: {error}")
        });
    let commodity = CommodityKey::new(requirement.material(), form);
    let lot = deposit_lot_for_fixture(
        registries,
        state,
        source,
        commodity,
        mass,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("gameplay bootstrap construction material failed: {error}"));
    let resolution = bind_structural_construction_selection(
        state,
        element,
        source,
        &[MaterialLotSelection::new(lot, mass)],
    )
    .unwrap_or_else(|error| panic!("gameplay bootstrap construction binding failed: {error:?}"));
    validate_structural_construction(registries, state, &resolution)
        .unwrap_or_else(|error| {
            panic!("gameplay bootstrap construction validation failed: {error}")
        })
        .commit(state)
        .unwrap_or_else(|error| panic!("gameplay bootstrap construction commit failed: {error}"));
}
