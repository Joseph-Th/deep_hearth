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

#[derive(Clone, Copy, Debug)]
pub(super) enum ChargePolicy {
    BatchDemand,
    FullBuffer,
}

pub(super) fn prepare_stage(
    registries: &Registries,
    state: &mut AppState,
    stage: (
        deep_hearth::production::ProcessId,
        EquipmentId,
        deep_hearth::inventory::StockpileId,
    ),
    power: (EquipmentId, EnergyStoreId),
    policy: ChargePolicy,
    label: &'static str,
) -> ChargeReport {
    let (process, equipment, feed) = stage;
    let (treadle, drive) = power;
    let store = state
        .energy()
        .get_store(drive)
        .expect("liberation drive exists");
    let stored = store.stored();
    let capacity = registries
        .energy()
        .get_store(store.definition())
        .expect("drive definition exists")
        .capacity();
    let mass = state
        .inventory()
        .get_stockpile(feed)
        .expect("stage feed exists")
        .stored_mass();
    let envelope = deep_hearth::ore_processing::assess_powered_ore_mass_envelope(
        registries, state, process, equipment, drive,
    )
    .unwrap_or_else(|error| panic!("{label} planning failed: {error}"));
    let shortfall = envelope
        .additional_energy_required_for(mass)
        .unwrap_or_else(|| panic!("{label} cannot fit its provider even after charging"));
    let requested = match policy {
        ChargePolicy::BatchDemand => shortfall,
        ChargePolicy::FullBuffer => capacity.checked_sub(stored).expect("store within capacity"),
    };
    if requested.is_zero() {
        return ChargeReport::zero(stored);
    }
    let before = deep_hearth::survival::assess_survival(registries, state)
        .expect("liberation player exists before charging");
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
    let ticks = finish_manual_power_work(registries, state, work, label);
    let stored_after = state
        .energy()
        .get_store(drive)
        .map(|record| record.stored())
        .unwrap_or_else(|| panic!("{label} drive disappeared after recharge"));
    assert!(
        stored_after > stored,
        "{label} recharge must increase stored mechanical work"
    );
    let after = deep_hearth::survival::assess_survival(registries, state)
        .expect("liberation player exists after charging");
    assert!(
        deep_hearth::ore_processing::assess_powered_ore_mass_envelope(
            registries, state, process, equipment, drive,
        )
        .expect("stage remains assessable after charging")
        .maximum_mass()
            >= mass,
        "{label} charge must fund the actual batch"
    );
    ChargeReport {
        requested,
        stored_before: stored,
        stored_after,
        ticks,
        metabolic_nj: before.metabolic_energy().nanojoules()
            - after.metabolic_energy().nanojoules(),
        hydration_ul: before.hydration().microliters() - after.hydration().microliters(),
    }
}

/// Executed cost of one committed manual-power charge, returned by the canonical completion path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ChargeReport {
    pub(super) requested: Energy,
    pub(super) stored_before: Energy,
    pub(super) stored_after: Energy,
    pub(super) ticks: u64,
    pub(super) metabolic_nj: u128,
    pub(super) hydration_ul: u64,
}

impl ChargeReport {
    pub(super) fn zero(stored: Energy) -> Self {
        Self {
            requested: Energy::ZERO,
            stored_before: stored,
            stored_after: stored,
            ticks: 0,
            metabolic_nj: 0,
            hydration_ul: 0,
        }
    }
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
