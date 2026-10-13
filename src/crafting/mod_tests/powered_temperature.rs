//! Thermal-cohort contracts shared by powered shaping and persistence replay.

use super::*;
use crate::content::{
    ENERGY_MECHANICAL_SMALL_DRIVE, EQUIPMENT_TIMBER_SASH_SAWMILL, FORM_BOARD, FORM_CHIP,
    FORM_HANDLE, FORM_LOG, FORM_REINFORCEMENT, FORM_SAW_BLADE, MATERIAL_COPPER, MATERIAL_WOOD,
    PROCESS_POWER_SAW_WOOD_BOARDS, build_registries,
};
use crate::core::quantity::{Energy, Mass, Temperature};
use crate::core::state::{AppState, validate_loaded_state};
use crate::energy::add_energy_store_with_initial_for_fixture;
use crate::equipment::validate_assemble_equipment;
use crate::inventory::{MaterialLotSelection, add_solid_stockpile_for_test, deposit_lot_for_test};
use crate::material::CommodityKey;

fn stockpile(state: &mut AppState, capacity_mg: u64) -> crate::inventory::StockpileId {
    add_solid_stockpile_for_test(state, Mass::from_milligrams(capacity_mg))
        .unwrap_or_else(|error| panic!("powered thermal-cohort stockpile failed: {error}"))
}

fn deposit(
    registries: &crate::registry::Registries,
    state: &mut AppState,
    stockpile: crate::inventory::StockpileId,
    commodity: CommodityKey,
    mass_mg: u64,
    temperature_mk: u32,
) -> crate::inventory::MaterialLotId {
    deposit_lot_for_test(
        registries,
        state,
        stockpile,
        commodity,
        Mass::from_milligrams(mass_mg),
        Temperature::from_millikelvin(temperature_mk),
    )
    .unwrap_or_else(|error| panic!("powered thermal-cohort material failed: {error}"))
}

#[test]
fn powered_shaping_preserves_mixed_temperature_cohorts_through_replay_validation() {
    let registries = build_registries();
    let mut state = AppState::new();
    let assembly = stockpile(&mut state, 6_000_000);
    for (commodity, mass) in [
        (CommodityKey::new(MATERIAL_WOOD, FORM_BOARD), 4_000_000),
        (CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE), 800_000),
        (CommodityKey::new(MATERIAL_COPPER, FORM_SAW_BLADE), 54_000),
        (
            CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
            20_000,
        ),
    ] {
        let _ = deposit(&registries, &mut state, assembly, commodity, mass, 293_150);
    }
    let sawmill =
        validate_assemble_equipment(&registries, &state, EQUIPMENT_TIMBER_SASH_SAWMILL, assembly)
            .unwrap_or_else(|error| {
                panic!("powered thermal-cohort sawmill assembly failed: {error}")
            })
            .commit(&mut state)
            .unwrap_or_else(|error| {
                panic!("powered thermal-cohort sawmill assembly commit failed: {error}")
            });

    let source = stockpile(&mut state, 1_000_000);
    let cool = deposit(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        600_000,
        293_150,
    );
    let warm = deposit(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        400_000,
        313_150,
    );
    let destination = stockpile(&mut state, 1_000_000);
    let drive = add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        ENERGY_MECHANICAL_SMALL_DRIVE,
        Energy::from_nanojoules(1_000_000_000_000),
    )
    .unwrap_or_else(|error| panic!("powered thermal-cohort drive failed: {error}"));
    let request = PoweredCraftRequest::new(
        PROCESS_POWER_SAW_WOOD_BOARDS,
        source,
        vec![
            MaterialLotSelection::new(cool, Mass::from_milligrams(600_000)),
            MaterialLotSelection::new(warm, Mass::from_milligrams(400_000)),
        ],
        sawmill,
        drive,
    );

    let resolution = resolve_powered_craft(&registries, &state, &request)
        .unwrap_or_else(|error| panic!("powered mixed-temperature shaping failed: {error}"));
    for (temperature, expected_mass) in [(293_150, 600_000), (313_150, 400_000)] {
        let temperature = Temperature::from_millikelvin(temperature);
        let mass = resolution
            .outputs()
            .iter()
            .filter(|output| output.temperature() == temperature)
            .fold(Mass::ZERO, |total, output| {
                total
                    .checked_add(output.mass())
                    .unwrap_or_else(|| panic!("powered thermal-cohort output mass overflowed"))
            });
        assert_eq!(mass, Mass::from_milligrams(expected_mass));
        let board_mass = resolution
            .outputs()
            .iter()
            .find(|output| {
                output.commodity() == CommodityKey::new(MATERIAL_WOOD, FORM_BOARD)
                    && output.temperature() == temperature
            })
            .map(crate::material::MaterialLotSpec::mass)
            .unwrap_or_else(|| panic!("powered board thermal cohort disappeared"));
        let chip_mass = resolution
            .outputs()
            .iter()
            .find(|output| {
                output.commodity() == CommodityKey::new(MATERIAL_WOOD, FORM_CHIP)
                    && output.temperature() == temperature
            })
            .map(crate::material::MaterialLotSpec::mass)
            .unwrap_or_else(|| panic!("powered chip thermal cohort disappeared"));
        let (expected_board, expected_chip) = if expected_mass == 600_000 {
            (540_000, 60_000)
        } else {
            (360_000, 40_000)
        };
        assert_eq!(board_mass, Mass::from_milligrams(expected_board));
        assert_eq!(chip_mass, Mass::from_milligrams(expected_chip));
    }

    let _job = crate::production::validate_start_process(
        &registries,
        &state,
        &resolution,
        source,
        destination,
    )
    .unwrap_or_else(|error| {
        panic!("powered mixed-temperature capability admission failed: {error}")
    })
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("powered mixed-temperature commit failed: {error}"));
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("powered mixed-temperature replay failed: {error}"));
}
