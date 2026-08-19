//! Explicit fixture setup for the ore-preparation capability probe.

use super::capability_boundary::{
    assert_capability_only_energy_store, assert_capability_only_equipment,
};
use super::support::{ROOM_TEMPERATURE, add_solid_stockpile};
use deep_hearth::content::gameplay_fixture::{
    seed_composed_lot, seed_energy_store as seed_energy_store_exact,
};
use deep_hearth::content::{
    ENERGY_MECHANICAL_LARGE_DRIVE, EQUIPMENT_DRY_SCREEN, EQUIPMENT_GRINDING_MILL,
    EQUIPMENT_JAW_CRUSHER, FORM_ORE, MATERIAL_COPPER, MATERIAL_STONE,
};
use deep_hearth::core::quantity::Mass;
use deep_hearth::core::state::AppState;
use deep_hearth::core::time::WorldSeed;
use deep_hearth::energy::EnergyStoreId;
use deep_hearth::equipment::{EquipmentId, add_equipment};
use deep_hearth::inventory::{MaterialLotId, StockpileId};
use deep_hearth::maintenance::Condition;
use deep_hearth::material::{CommodityKey, CompositionComponent, MaterialComposition};
use deep_hearth::registry::Registries;

pub(super) fn mixed_ore_composition(copper_ppm: u32) -> MaterialComposition {
    MaterialComposition::new(vec![
        CompositionComponent::new(MATERIAL_COPPER, copper_ppm),
        CompositionComponent::new(MATERIAL_STONE, 1_000_000 - copper_ppm),
    ])
    .unwrap_or_else(|error| panic!("gameplay harness ore composition failed: {error}"))
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

pub(super) fn setup_ore_preparation_probe(
    registries: &Registries,
    seed: u64,
    batch_mass: Mass,
    copper_ppm: u32,
) -> (AppState, OrePreparationProbeIds) {
    for equipment in [
        EQUIPMENT_JAW_CRUSHER,
        EQUIPMENT_GRINDING_MILL,
        EQUIPMENT_DRY_SCREEN,
    ] {
        assert_capability_only_equipment(registries, equipment);
    }
    assert_capability_only_energy_store(registries, ENERGY_MECHANICAL_LARGE_DRIVE);
    let mut state = AppState::new(WorldSeed::new(seed));
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
