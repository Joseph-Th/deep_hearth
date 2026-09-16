//! Shared primitive-liberation harness operations for assembly, charging, and exact stock selection.

use deep_hearth::content::gameplay_fixture::seed_lot;
use deep_hearth::content::{MANUAL_POWER_FOOT_TREADLE, MATERIAL_COPPER};
use deep_hearth::core::quantity::Energy;
use deep_hearth::core::state::AppState;
use deep_hearth::energy::{EnergyStoreId, validate_assemble_energy_store};
use deep_hearth::equipment::{EquipmentId, validate_assemble_equipment};
use deep_hearth::inventory::MaterialLotSelection;
use deep_hearth::labor::{ManualPowerRequest, validate_start_manual_power};
use deep_hearth::registry::Registries;

use super::super::environment::ROOM_TEMPERATURE;
use super::super::inventory_support::add_solid_stockpile;
use super::super::manual_power_timing::finish_manual_power_work;

pub(super) fn assemble_equipment_from_authored_parts(
    registries: &Registries,
    state: &mut AppState,
    definition: deep_hearth::equipment::EquipmentDefinitionId,
) -> EquipmentId {
    let (mass, inputs) = registries
        .equipment()
        .get_equipment(definition)
        .and_then(|equipment| equipment.assembly_profile())
        .map(|profile| (profile.input_mass(), profile.inputs().to_vec()))
        .unwrap_or_else(|| panic!("primitive liberation equipment lost authored assembly"));
    let source = add_solid_stockpile(state, mass);
    for input in inputs {
        seed_lot(
            registries,
            state,
            source,
            input.commodity(),
            input.mass(),
            ROOM_TEMPERATURE,
        );
    }
    validate_assemble_equipment(registries, state, definition, source)
        .unwrap_or_else(|error| panic!("primitive liberation equipment assembly failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("primitive liberation equipment commit failed: {error}"))
}

pub(super) fn assemble_energy_store_from_authored_parts(
    registries: &Registries,
    state: &mut AppState,
    definition: deep_hearth::energy::EnergyStoreDefinitionId,
) -> EnergyStoreId {
    let (mass, inputs) = registries
        .energy()
        .get_store(definition)
        .and_then(|store| store.assembly_profile())
        .map(|profile| (profile.input_mass(), profile.inputs().to_vec()))
        .unwrap_or_else(|| panic!("primitive liberation drive lost authored assembly"));
    let source = add_solid_stockpile(state, mass);
    for input in inputs {
        seed_lot(
            registries,
            state,
            source,
            input.commodity(),
            input.mass(),
            ROOM_TEMPERATURE,
        );
    }
    validate_assemble_energy_store(registries, state, definition, source)
        .unwrap_or_else(|error| panic!("primitive liberation drive assembly failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("primitive liberation drive commit failed: {error}"))
}

pub(super) fn replenish_primitive_drive(
    registries: &Registries,
    state: &mut AppState,
    treadle: EquipmentId,
    drive: EnergyStoreId,
    capacity: Energy,
    label: &'static str,
) {
    let stored = state
        .energy()
        .get_store(drive)
        .map(|record| record.stored())
        .unwrap_or_else(|| panic!("{label} drive disappeared"));
    let requested = capacity
        .checked_sub(stored)
        .unwrap_or_else(|| panic!("{label} drive exceeded authored capacity"));
    if requested.is_zero() {
        return;
    }
    let charge = validate_start_manual_power(
        registries,
        state,
        ManualPowerRequest::new(MANUAL_POWER_FOOT_TREADLE, treadle, drive, requested),
    )
    .unwrap_or_else(|error| panic!("{label} treadle recharge failed: {error}"));
    let work = charge.work();
    charge
        .commit(state)
        .unwrap_or_else(|error| panic!("{label} treadle recharge commit failed: {error}"));
    finish_manual_power_work(registries, state, work, label);
    assert!(
        state
            .energy()
            .get_store(drive)
            .is_some_and(|record| record.stored() > stored),
        "{label} recharge must increase stored mechanical work"
    );
}

pub(super) fn full_stockpile_selection(
    state: &AppState,
    stockpile: deep_hearth::inventory::StockpileId,
) -> Vec<MaterialLotSelection> {
    state
        .inventory()
        .lot_ids(stockpile)
        .map(|lot| {
            let mass = state
                .inventory()
                .get_lot(lot)
                .unwrap_or_else(|| panic!("full-selection lot disappeared"))
                .mass();
            MaterialLotSelection::new(lot, mass)
        })
        .collect()
}

pub(super) fn copper_numerator_ppm_mg(
    state: &AppState,
    stockpile: deep_hearth::inventory::StockpileId,
) -> u128 {
    state
        .inventory()
        .lot_ids(stockpile)
        .map(|lot| {
            let record = state
                .inventory()
                .get_lot(lot)
                .unwrap_or_else(|| panic!("copper-accounting lot disappeared"));
            u128::from(record.mass().milligrams())
                * u128::from(record.composition().parts_per_million(MATERIAL_COPPER))
        })
        .sum()
}
