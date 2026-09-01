//! Explicit fixture setup for the ore-preparation capability probe.

use super::capability_boundary::{
    seed_capability_only_energy_store, seed_capability_only_equipment,
};
use super::environment::ROOM_TEMPERATURE;
use super::industrial_support::install_equipment_on_grounded_support;
use super::inventory_support::add_solid_stockpile;
use super::ore_fixture::copper_ore_composition;
use deep_hearth::content::gameplay_fixture::seed_composed_lot;
use deep_hearth::content::{
    ENERGY_MECHANICAL_LARGE_DRIVE, EQUIPMENT_DRY_SCREEN, EQUIPMENT_GRAVITY_SEPARATOR,
    EQUIPMENT_GRINDING_MILL, EQUIPMENT_JAW_CRUSHER, FORM_ORE, MATERIAL_COPPER,
};
use deep_hearth::core::quantity::{Energy, Mass};
use deep_hearth::core::state::AppState;
use deep_hearth::core::time::WorldSeed;
use deep_hearth::energy::EnergyStoreId;
use deep_hearth::equipment::EquipmentId;
use deep_hearth::inventory::{MaterialLotId, StockpileId};
use deep_hearth::maintenance::Condition;
use deep_hearth::material::CommodityKey;
use deep_hearth::registry::Registries;

#[derive(Clone, Copy)]
pub(super) struct OrePreparationProbeIds {
    pub(super) ore_source: StockpileId,
    pub(super) crushed_storage: StockpileId,
    pub(super) ground_storage: StockpileId,
    pub(super) undersize_storage: StockpileId,
    pub(super) oversize_storage: StockpileId,
    pub(super) concentrate_storage: StockpileId,
    pub(super) tailings_storage: StockpileId,
    pub(super) ore_lot: MaterialLotId,
    pub(super) crusher: EquipmentId,
    pub(super) grinder: EquipmentId,
    pub(super) screen: EquipmentId,
    pub(super) separator: EquipmentId,
    pub(super) drive: EnergyStoreId,
}

#[derive(Clone, Copy)]
pub(super) struct OrePreparationSetup {
    pub(super) batch_mass: Mass,
    pub(super) representable_unit_mg: u64,
    pub(super) copper_ppm: u32,
    pub(super) clay_share_ppm: u32,
    pub(super) crusher_condition: Condition,
    pub(super) grinder_condition: Condition,
    pub(super) screen_condition: Condition,
    pub(super) separator_condition: Condition,
    pub(super) drive_energy: Energy,
}

pub(super) fn setup_ore_preparation_probe(
    registries: &Registries,
    seed: u64,
    setup: OrePreparationSetup,
) -> (AppState, OrePreparationProbeIds) {
    let OrePreparationSetup {
        batch_mass,
        representable_unit_mg: _,
        copper_ppm,
        clay_share_ppm,
        crusher_condition,
        grinder_condition,
        screen_condition,
        separator_condition,
        drive_energy,
    } = setup;
    let mut state = AppState::new(WorldSeed::new(seed));
    let ore_source = add_solid_stockpile(&mut state, batch_mass);
    let crushed_storage = add_solid_stockpile(&mut state, batch_mass);
    let ground_storage = add_solid_stockpile(&mut state, batch_mass);
    let undersize_storage = add_solid_stockpile(&mut state, batch_mass);
    let oversize_storage = add_solid_stockpile(&mut state, batch_mass);
    let concentrate_storage = add_solid_stockpile(&mut state, batch_mass);
    let tailings_storage = add_solid_stockpile(&mut state, batch_mass);
    let ore_lot = seed_composed_lot(
        registries,
        &mut state,
        ore_source,
        CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
        batch_mass,
        ROOM_TEMPERATURE,
        copper_ore_composition(copper_ppm, clay_share_ppm),
    );
    let crusher = seed_capability_only_equipment(
        registries,
        &mut state,
        EQUIPMENT_JAW_CRUSHER,
        crusher_condition,
    );
    let grinder = seed_capability_only_equipment(
        registries,
        &mut state,
        EQUIPMENT_GRINDING_MILL,
        grinder_condition,
    );
    let screen = seed_capability_only_equipment(
        registries,
        &mut state,
        EQUIPMENT_DRY_SCREEN,
        screen_condition,
    );
    let separator = seed_capability_only_equipment(
        registries,
        &mut state,
        EQUIPMENT_GRAVITY_SEPARATOR,
        separator_condition,
    );
    install_equipment_on_grounded_support(registries, &mut state, crusher, 0);
    install_equipment_on_grounded_support(registries, &mut state, grinder, 2);
    install_equipment_on_grounded_support(registries, &mut state, screen, 4);
    install_equipment_on_grounded_support(registries, &mut state, separator, 6);
    let drive = seed_capability_only_energy_store(
        registries,
        &mut state,
        ENERGY_MECHANICAL_LARGE_DRIVE,
        drive_energy,
    );
    (
        state,
        OrePreparationProbeIds {
            ore_source,
            crushed_storage,
            ground_storage,
            undersize_storage,
            oversize_storage,
            concentrate_storage,
            tailings_storage,
            ore_lot,
            crusher,
            grinder,
            screen,
            separator,
            drive,
        },
    )
}
