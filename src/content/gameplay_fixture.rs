//! External starting-state boundary for the gameplay exercise.
//!
//! Deep Hearth does not yet own world acquisition for loose matter, potable fluid, stored-energy
//! generation, or a player-facing structural construction authorizer. The gameplay harness therefore
//! has to arrange those facts before the acting policy starts. Keep every direct bootstrap-only
//! mutation in this module so the exercise itself cannot accidentally treat a fixture shortcut as
//! player behavior.
//!
//! Once setup returns, gameplay code must use the same runtime resolvers, validators, commits, and
//! simulation ticks as the game core.

use crate::core::quantity::{Energy, Mass, Pressure, Temperature, Volume};
use crate::core::state::AppState;
use crate::energy::{
    EnergyStoreDefinitionId, EnergyStoreId, add_energy_store_with_initial_for_fixture,
};
use crate::equipment::{EquipmentDefinitionId, EquipmentId, add_equipment};
use crate::fluid::{FluidDefinitionId, FluidStoreId, add_fluid_store_with_contents_for_fixture};
use crate::geology::{GeneratedDepositSpec, GeologicalDepositId, insert_generated_deposit};
use crate::inventory::{
    MaterialLotId, MaterialLotSelection, MaterialTransferResolution, StockpileId,
    StockpileStorageProfile, add_stockpile, deposit_composed_lot_for_fixture,
    deposit_lot_for_fixture,
};
use crate::maintenance::Condition;
use crate::material::{CommodityKey, FormId, MaterialComposition};
use crate::registry::Registries;
use crate::spatial::VoxelBounds;
use crate::structural::{
    StructuralElementId, bind_structural_construction_selection,
    resolve_structural_material_requirement, validate_structural_construction,
};
use crate::survival::{
    initialize_player_survival_at_hunger_warning_for_fixture,
    initialize_player_survival_at_hydration_warning_for_fixture,
};

/// Seeds a controlled scenario player at the authored hydration warning boundary.
///
/// This represents a pre-existing starting condition for the maintained survival-pressure world. It
/// does not advance the simulation or provide an acting-policy shortcut once setup has completed.
pub fn seed_player_survival_at_hydration_warning(registries: &Registries, state: &mut AppState) {
    initialize_player_survival_at_hydration_warning_for_fixture(registries, state).unwrap_or_else(
        |error| panic!("gameplay bootstrap hydration-warning seed failed: {error}"),
    );
}

/// Seeds a controlled scenario player at the authored hunger warning boundary.
///
/// This represents a pre-existing starting condition for a maintained survival-pressure world. It
/// does not advance the simulation or provide an acting-policy shortcut once setup has completed.
pub fn seed_player_survival_at_hunger_warning(registries: &Registries, state: &mut AppState) {
    initialize_player_survival_at_hunger_warning_for_fixture(registries, state)
        .unwrap_or_else(|error| panic!("gameplay bootstrap hunger-warning seed failed: {error}"));
}

pub fn seed_stockpile(
    state: &mut AppState,
    capacity: Mass,
    storage_profile: StockpileStorageProfile,
) -> StockpileId {
    add_stockpile(state, capacity, storage_profile)
        .unwrap_or_else(|error| panic!("gameplay bootstrap stockpile seed failed: {error}"))
}

pub fn seed_equipment(
    registries: &Registries,
    state: &mut AppState,
    definition: EquipmentDefinitionId,
    condition: Condition,
) -> EquipmentId {
    add_equipment(registries, state, definition, condition)
        .unwrap_or_else(|error| panic!("gameplay bootstrap equipment seed failed: {error}"))
}

pub fn seed_energy_store(
    registries: &Registries,
    state: &mut AppState,
    definition: EnergyStoreDefinitionId,
    amount: Energy,
) -> EnergyStoreId {
    add_energy_store_with_initial_for_fixture(registries, state, definition, amount)
        .unwrap_or_else(|error| panic!("gameplay bootstrap energy seed failed: {error}"))
}

pub fn seed_fluid_store(
    registries: &Registries,
    state: &mut AppState,
    capacity: Volume,
    definition: FluidDefinitionId,
    volume: Volume,
    temperature: Temperature,
) -> FluidStoreId {
    add_fluid_store_with_contents_for_fixture(
        registries,
        state,
        capacity,
        definition,
        volume,
        temperature,
    )
    .unwrap_or_else(|error| panic!("gameplay bootstrap fluid seed failed: {error}"))
}

pub fn geological_deposit_spec(
    bounds: VoxelBounds,
    commodity: CommodityKey,
    mass: Mass,
    temperature: Temperature,
    excavation_hardness: Pressure,
    composition: MaterialComposition,
) -> GeneratedDepositSpec {
    GeneratedDepositSpec::new(
        bounds,
        commodity,
        mass,
        temperature,
        excavation_hardness,
        composition,
    )
    .unwrap_or_else(|error| panic!("gameplay bootstrap geological specification failed: {error}"))
}

pub fn seed_geological_deposit(
    registries: &Registries,
    state: &mut AppState,
    spec: GeneratedDepositSpec,
) -> GeologicalDepositId {
    insert_generated_deposit(registries, state, spec)
        .unwrap_or_else(|error| panic!("gameplay bootstrap geological deposit failed: {error}"))
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

/// Creates the harness-only logistics authorization for one controlled material-delivery event.
///
/// Call this during scenario setup, before the acting policy starts. The fixture does not move matter
/// or reveal event timing to the actor. Inventory still validates and commits the canonical transfer;
/// this is a controlled audit authorization because world logistics is outside current production
/// scope and ordinary runtime cannot create pathless transfers.
pub const fn authorize_controlled_material_delivery(
    source: StockpileId,
    destination: StockpileId,
    commodity: CommodityKey,
    mass: Mass,
) -> MaterialTransferResolution {
    MaterialTransferResolution::new(source, destination, commodity, mass)
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
    let source = seed_stockpile(state, mass, StockpileStorageProfile::solid_only());
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
    validate_structural_construction(registries, state, resolution)
        .unwrap_or_else(|error| {
            panic!("gameplay bootstrap construction validation failed: {error}")
        })
        .commit(state)
        .unwrap_or_else(|error| panic!("gameplay bootstrap construction commit failed: {error}"));
}
