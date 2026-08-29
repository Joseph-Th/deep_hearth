//! Controlled starting-state construction for gameplay evaluation.
//!
//! General loose-matter/fluid acquisition, broad construction/haulage, and acquisition for every
//! industrial energy carrier are outside current runtime scope. This module may establish only those
//! unavailable prerequisites before actor admission. Reachable mechanics, including primitive
//! energy-store assembly and survival-costed manual generation, use their canonical runtime paths.
//!
//! After setup, gameplay evaluation uses production resolvers, validators, commits, and simulation
//! ticks. Setup-only mutation stays in this module so fixture authority cannot become actor authority.

use crate::core::quantity::{Energy, Mass, Pressure, Temperature, Volume};
use crate::core::state::{AppState, apply_clock_advance};
use crate::core::time::SimulationTick;
use crate::energy::{
    EnergyStoreDefinitionId, EnergyStoreId, add_energy_store_with_initial_for_fixture,
};
use crate::equipment::{EquipmentDefinitionId, EquipmentId, add_equipment};
use crate::fluid::{FluidDefinitionId, FluidStoreId, add_fluid_store_with_contents_for_fixture};
use crate::geology::{GeneratedDepositSpec, insert_generated_deposit};
use crate::inventory::{
    MaterialLotId, MaterialLotSelection, MaterialTransferResolution, StockpileId,
    StockpileStorageProfile, add_stockpile, deposit_composed_lot_for_fixture,
    deposit_lot_for_fixture,
};
use crate::maintenance::Condition;
use crate::material::{CommodityKey, FormId, MaterialComposition, MaterialId};
use crate::registry::Registries;
use crate::spatial::VoxelBounds;
use crate::structural::{
    StructuralElementGeometry, StructuralElementId, StructuralProfileId, add_structural_element,
    bind_structural_construction_selection, resolve_structural_material_requirement,
    validate_activate_structural_element, validate_structural_construction,
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

/// Advances setup-only world age without executing gameplay systems.
///
/// Use this only to represent material or infrastructure that already existed before the acting
/// episode begins. This setup path requires an uninitialized player so it cannot erase survival cost
/// or skip active player work. Once the player is initialized, time must advance through canonical
/// simulation ticks.
pub fn seed_preexisting_world_age(state: &mut AppState, tick: SimulationTick) {
    assert_eq!(
        state.tick(),
        SimulationTick::ZERO,
        "gameplay bootstrap world age may only be established from the initial tick"
    );
    assert!(
        state.survival().player().is_none(),
        "gameplay bootstrap world age must be established before player survival begins"
    );
    apply_clock_advance(state, tick);
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

/// Inert authored facts for one geological deposit used only by the controlled gameplay fixture.
///
/// This value is not a generation authorization. The crate-private `GeneratedDepositSpec` remains
/// the validating authority created only inside [`seed_geological_deposit`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeologicalDepositSeed {
    bounds: VoxelBounds,
    commodity: CommodityKey,
    mass: Mass,
    temperature: Temperature,
    excavation_hardness: Pressure,
    composition: MaterialComposition,
}

impl GeologicalDepositSeed {
    #[must_use]
    pub const fn new(
        bounds: VoxelBounds,
        commodity: CommodityKey,
        mass: Mass,
        temperature: Temperature,
        excavation_hardness: Pressure,
        composition: MaterialComposition,
    ) -> Self {
        Self {
            bounds,
            commodity,
            mass,
            temperature,
            excavation_hardness,
            composition,
        }
    }
}

/// Seeds one pre-existing geological deposit through the crate-owned generation boundary.
///
/// The harness supplies only authored physical facts. The internal generation specification and its
/// validator remain crate-private; this function is the external controlled-setup mutation boundary.
pub fn seed_geological_deposit(
    registries: &Registries,
    state: &mut AppState,
    seed: GeologicalDepositSeed,
) {
    let GeologicalDepositSeed {
        bounds,
        commodity,
        mass,
        temperature,
        excavation_hardness,
        composition,
    } = seed;
    let spec = GeneratedDepositSpec::new(
        bounds,
        commodity,
        mass,
        temperature,
        excavation_hardness,
        composition,
    )
    .unwrap_or_else(|error| panic!("gameplay bootstrap geological specification failed: {error}"));
    let _deposit = insert_generated_deposit(registries, state, spec)
        .unwrap_or_else(|error| panic!("gameplay bootstrap geological deposit failed: {error}"));
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

fn materialize_structure(
    registries: &Registries,
    state: &mut AppState,
    element: StructuralElementId,
    form: FormId,
) {
    let requirement = resolve_structural_material_requirement(registries, state, element)
        .unwrap_or_else(|error| panic!("gameplay bootstrap material requirement failed: {error}"));
    let mass = requirement.required_mass();
    let source = seed_stockpile(state, mass, StockpileStorageProfile::unbounded_solid_only());
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

/// Creates, materializes, and activates one grounded structural member during controlled setup.
///
/// The low-level structural allocation and activation operations remain crate-private so the gameplay
/// harness cannot use them after actor admission. This fixture is the external setup boundary.
pub fn seed_grounded_active_structure(
    registries: &Registries,
    state: &mut AppState,
    profile: StructuralProfileId,
    material: MaterialId,
    geometry: StructuralElementGeometry,
    form: FormId,
) -> StructuralElementId {
    let element = add_structural_element(registries, state, profile, material, geometry, true)
        .unwrap_or_else(|error| panic!("gameplay bootstrap structural allocation failed: {error}"));
    materialize_structure(registries, state, element, form);
    let _ = validate_activate_structural_element(registries, state, element)
        .unwrap_or_else(|error| panic!("gameplay bootstrap structural activation failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| {
            panic!("gameplay bootstrap structural activation commit failed: {error}")
        });
    element
}
