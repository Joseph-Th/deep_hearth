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
    EnergyStoreDefinitionId, EnergyStoreId, add_energy_store_with_initial_for_test,
};
use crate::inventory::{
    MaterialLotId, StockpileId, deposit_composed_lot_for_test, deposit_lot_for_test,
};
use crate::material::{CommodityKey, FormId, MaterialComposition};
use crate::registry::Registries;
use crate::structural::{StructuralElementId, materialize_structural_element_for_test};

pub(super) fn seed_energy_store(
    registries: &Registries,
    state: &mut AppState,
    definition: EnergyStoreDefinitionId,
    amount: Energy,
) -> EnergyStoreId {
    add_energy_store_with_initial_for_test(registries, state, definition, amount)
        .unwrap_or_else(|error| panic!("gameplay bootstrap energy seed failed: {error}"))
}

pub(super) fn seed_lot(
    registries: &Registries,
    state: &mut AppState,
    stockpile: StockpileId,
    commodity: CommodityKey,
    mass: Mass,
    temperature: Temperature,
) -> MaterialLotId {
    deposit_lot_for_test(registries, state, stockpile, commodity, mass, temperature)
        .unwrap_or_else(|error| panic!("gameplay bootstrap material seed failed: {error}"))
}

pub(super) fn seed_composed_lot(
    registries: &Registries,
    state: &mut AppState,
    stockpile: StockpileId,
    commodity: CommodityKey,
    mass: Mass,
    temperature: Temperature,
    composition: MaterialComposition,
) -> MaterialLotId {
    deposit_composed_lot_for_test(
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

pub(super) fn materialize_structure(
    registries: &Registries,
    state: &mut AppState,
    element: StructuralElementId,
    form: FormId,
) {
    materialize_structural_element_for_test(registries, state, element, form);
}
