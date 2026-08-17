//! Explicit fixture setup for gameplay-harness ore-preparation and foundry capability probes.

use super::{
    ENERGY_ELECTRICAL_BUFFER, ENERGY_MECHANICAL_LARGE_DRIVE, ENERGY_THERMAL_SINK,
    EQUIPMENT_CASTING_MOLD, EQUIPMENT_DRY_SCREEN, EQUIPMENT_ELECTRIC_FURNACE,
    EQUIPMENT_GRINDING_MILL, EQUIPMENT_JAW_CRUSHER, FORM_INGOT, FORM_ORE, MATERIAL_COPPER,
    ROOM_TEMPERATURE, add_solid_stockpile, mixed_ore_composition, seed_composed_lot,
    seed_energy_store_exact, seed_lot,
};
use deep_hearth::core::quantity::{Mass, Temperature};
use deep_hearth::core::state::AppState;
use deep_hearth::core::time::WorldSeed;
use deep_hearth::energy::{EnergyStoreId, add_energy_store};
use deep_hearth::equipment::{EquipmentId, add_equipment};
use deep_hearth::inventory::{MaterialLotId, StockpileId, StockpileStorageProfile, add_stockpile};
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
pub(super) struct OrePreparationProbeIds {
    pub(super) ore_source: StockpileId,
    pub(super) crushed_storage: StockpileId,
    pub(super) ground_storage: StockpileId,
    pub(super) undersize_storage: StockpileId,
    pub(super) oversize_storage: StockpileId,
    pub(super) ore_lot: MaterialLotId,
    pub(super) crusher: EquipmentId,
    pub(super) grinder: EquipmentId,
    pub(super) screen: EquipmentId,
    pub(super) drive: EnergyStoreId,
}

pub(super) fn setup_foundry_probe(registries: &Registries, mass: Mass) -> (AppState, FoundryIds) {
    let mut state = AppState::new(WorldSeed::new(0xD33F_F001));
    let pure_copper_source = add_solid_stockpile(&mut state, mass, "foundry copper source");
    let vessel_profile =
        StockpileStorageProfile::new(false, true, Temperature::from_millikelvin(1_500_000))
            .unwrap_or_else(|error| panic!("foundry probe molten storage profile failed: {error}"));
    let molten_vessel = add_stockpile(&mut state, mass, vessel_profile)
        .unwrap_or_else(|error| panic!("foundry probe molten vessel failed: {error}"));
    let cast_storage = add_solid_stockpile(&mut state, mass, "foundry cast storage");
    let pure_copper_lot = seed_lot(
        registries,
        &mut state,
        pure_copper_source,
        CommodityKey::new(MATERIAL_COPPER, FORM_INGOT),
        mass,
        ROOM_TEMPERATURE,
    );
    let furnace = add_equipment(
        registries,
        &mut state,
        EQUIPMENT_ELECTRIC_FURNACE,
        Condition::PRISTINE,
    )
    .unwrap_or_else(|error| panic!("foundry probe furnace allocation failed: {error}"));
    let mold = add_equipment(
        registries,
        &mut state,
        EQUIPMENT_CASTING_MOLD,
        Condition::PRISTINE,
    )
    .unwrap_or_else(|error| panic!("foundry probe mold allocation failed: {error}"));
    let electrical_capacity = registries
        .energy()
        .get_store(ENERGY_ELECTRICAL_BUFFER)
        .map(|definition| definition.capacity())
        .unwrap_or_else(|| panic!("foundry probe electrical-buffer definition disappeared"));
    let electrical_buffer = seed_energy_store_exact(
        registries,
        &mut state,
        ENERGY_ELECTRICAL_BUFFER,
        electrical_capacity,
    );
    let heat_sink = add_energy_store(registries, &mut state, ENERGY_THERMAL_SINK)
        .unwrap_or_else(|error| panic!("foundry probe thermal sink allocation failed: {error}"));
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

pub(super) fn setup_ore_preparation_probe(
    registries: &Registries,
    batch_mass: Mass,
    copper_ppm: u32,
) -> (AppState, OrePreparationProbeIds) {
    let mut state = AppState::new(WorldSeed::new(0xD33F_0A11));
    let ore_source = add_solid_stockpile(&mut state, batch_mass, "ore preparation source");
    let crushed_storage =
        add_solid_stockpile(&mut state, batch_mass, "ore preparation crushed storage");
    let ground_storage =
        add_solid_stockpile(&mut state, batch_mass, "ore preparation ground storage");
    let undersize_storage =
        add_solid_stockpile(&mut state, batch_mass, "ore preparation undersize storage");
    let oversize_storage =
        add_solid_stockpile(&mut state, batch_mass, "ore preparation oversize storage");
    let ore_lot = seed_composed_lot(
        registries,
        &mut state,
        ore_source,
        CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
        batch_mass,
        ROOM_TEMPERATURE,
        mixed_ore_composition(copper_ppm),
    );
    let crusher = add_equipment(
        registries,
        &mut state,
        EQUIPMENT_JAW_CRUSHER,
        Condition::PRISTINE,
    )
    .unwrap_or_else(|error| panic!("ore preparation crusher failed: {error}"));
    let grinder = add_equipment(
        registries,
        &mut state,
        EQUIPMENT_GRINDING_MILL,
        Condition::PRISTINE,
    )
    .unwrap_or_else(|error| panic!("ore preparation grinder failed: {error}"));
    let screen = add_equipment(
        registries,
        &mut state,
        EQUIPMENT_DRY_SCREEN,
        Condition::PRISTINE,
    )
    .unwrap_or_else(|error| panic!("ore preparation screen failed: {error}"));
    let drive_capacity = registries
        .energy()
        .get_store(ENERGY_MECHANICAL_LARGE_DRIVE)
        .map(|definition| definition.capacity())
        .unwrap_or_else(|| panic!("ore preparation drive definition disappeared"));
    let drive = seed_energy_store_exact(
        registries,
        &mut state,
        ENERGY_MECHANICAL_LARGE_DRIVE,
        drive_capacity,
    );
    (
        state,
        OrePreparationProbeIds {
            ore_source,
            crushed_storage,
            ground_storage,
            undersize_storage,
            oversize_storage,
            ore_lot,
            crusher,
            grinder,
            screen,
            drive,
        },
    )
}
