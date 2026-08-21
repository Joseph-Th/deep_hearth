//! Explicit fixture setup for the pure-copper foundry capability probe.

use super::capability_boundary::{
    assert_capability_only_energy_store, assert_capability_only_equipment,
};
use super::industrial_support::install_equipment_on_grounded_support;
use super::support::add_solid_stockpile;
use deep_hearth::content::gameplay_fixture::{
    seed_energy_store as seed_energy_store_exact, seed_equipment, seed_lot, seed_stockpile,
};
use deep_hearth::content::{
    ENERGY_ELECTRICAL_BUFFER, ENERGY_THERMAL_SINK, EQUIPMENT_CASTING_MOLD,
    EQUIPMENT_ELECTRIC_FURNACE, FORM_INGOT, MATERIAL_COPPER,
};
use deep_hearth::core::quantity::{Energy, Mass, Temperature};
use deep_hearth::core::state::AppState;
use deep_hearth::core::time::WorldSeed;
use deep_hearth::energy::EnergyStoreId;
use deep_hearth::equipment::EquipmentId;
use deep_hearth::inventory::{MaterialLotId, StockpileId, StockpileStorageProfile};
use deep_hearth::maintenance::Condition;
use deep_hearth::material::CommodityKey;
use deep_hearth::registry::Registries;

#[derive(Clone, Copy)]
pub(super) struct FoundryIds {
    pub(super) pure_copper_source: StockpileId,
    pub(super) molten_vessel: StockpileId,
    pub(super) cast_storage: StockpileId,
    pub(super) pure_copper_lot: MaterialLotId,
    pub(super) furnace: EquipmentId,
    pub(super) mold: EquipmentId,
    pub(super) electrical_buffer: EnergyStoreId,
    pub(super) heat_sink: EnergyStoreId,
}

#[derive(Clone, Copy)]
pub(super) struct FoundrySetup {
    pub(super) mass: Mass,
    pub(super) input_temperature: Temperature,
    pub(super) furnace_condition: Condition,
    pub(super) mold_condition: Condition,
    pub(super) electrical_energy: Energy,
}

pub(super) fn setup_foundry_probe(
    registries: &Registries,
    seed: u64,
    setup: FoundrySetup,
) -> (AppState, FoundryIds) {
    let FoundrySetup {
        mass,
        input_temperature,
        furnace_condition,
        mold_condition,
        electrical_energy,
    } = setup;
    for equipment in [EQUIPMENT_ELECTRIC_FURNACE, EQUIPMENT_CASTING_MOLD] {
        assert_capability_only_equipment(registries, equipment);
    }
    for store in [ENERGY_ELECTRICAL_BUFFER, ENERGY_THERMAL_SINK] {
        assert_capability_only_energy_store(registries, store);
    }
    let mut state = AppState::new(WorldSeed::new(seed));
    let pure_copper_source = add_solid_stockpile(&mut state, mass);
    let molten_temperature = registries
        .materials()
        .get_material(MATERIAL_COPPER)
        .and_then(|material| material.properties().thermal().melting_point())
        .unwrap_or_else(|| panic!("foundry probe copper fusion definition disappeared"));
    let vessel_profile = StockpileStorageProfile::new(false, true, molten_temperature)
        .unwrap_or_else(|error| panic!("foundry probe molten storage profile failed: {error}"));
    let molten_vessel = seed_stockpile(&mut state, mass, vessel_profile);
    let cast_storage = add_solid_stockpile(&mut state, mass);
    let pure_copper_lot = seed_lot(
        registries,
        &mut state,
        pure_copper_source,
        CommodityKey::new(MATERIAL_COPPER, FORM_INGOT),
        mass,
        input_temperature,
    );
    let furnace = seed_equipment(
        registries,
        &mut state,
        EQUIPMENT_ELECTRIC_FURNACE,
        furnace_condition,
    );
    let mold = seed_equipment(
        registries,
        &mut state,
        EQUIPMENT_CASTING_MOLD,
        mold_condition,
    );
    install_equipment_on_grounded_support(registries, &mut state, furnace, 0);
    install_equipment_on_grounded_support(registries, &mut state, mold, 2);
    let electrical_buffer = seed_energy_store_exact(
        registries,
        &mut state,
        ENERGY_ELECTRICAL_BUFFER,
        electrical_energy,
    );
    let heat_sink =
        seed_energy_store_exact(registries, &mut state, ENERGY_THERMAL_SINK, Energy::ZERO);
    (
        state,
        FoundryIds {
            pure_copper_source,
            molten_vessel,
            cast_storage,
            pure_copper_lot,
            furnace,
            mold,
            electrical_buffer,
            heat_sink,
        },
    )
}
