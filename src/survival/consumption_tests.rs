//! Contract tests for direct food and fluid consumption.

use super::*;
use crate::content::{
    FLUID_WATER, FORM_CHEST_BODY, FORM_FOOD, FORM_LUMP, MATERIAL_BERRIES, MATERIAL_GRAIN,
    MATERIAL_MEAT, MATERIAL_STONE, MATERIAL_WOOD, PROCESS_KNAP_STONE_TOOL,
    STORAGE_TIMBER_PROVISIONS_CHEST, build_registries,
};
use crate::core::quantity::{AggregateMass, AggregateVolume, Energy, Mass, Temperature, Volume};
use crate::core::state::{
    AppState, StateValidationError, apply_clock_advance, validate_loaded_state,
};
use crate::core::time::{SimulationTick, TickSpan, WorldSeed};
use crate::crafting::{ManualCraftStartRequest, validate_start_manual_craft};
use crate::fluid::{add_fluid_store_with_contents_for_fixture, calculate_fluid_volume_accounting};
use crate::inventory::{
    MaterialLotSelection, StockpileStorageProfile, add_solid_stockpile_for_test, add_stockpile,
    deposit_lot_for_test, validate_build_storage_enclosure, validate_material_transfer_for_test,
};
use crate::labor::{PlayerWork, PlayerWorkValidationError};
use crate::material::CommodityKey;
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
use crate::registry::Registries;
use crate::simulation::advance_tick;
use crate::survival::{
    FoodCategory, NUTRITION_PARTS_PER_MILLION, NutritionReserves, SurvivalValidationError,
    Vitality, assess_survival, initialize_player_survival, player_record,
};

fn initialize_and_spend_reserves(registries: &Registries, state: &mut AppState) {
    initialize_player_survival(registries, state)
        .unwrap_or_else(|error| panic!("survival initialization failed: {error}"));
    for _ in 0..5 {
        let _ = advance_tick(registries, state)
            .unwrap_or_else(|error| panic!("survival reserve-spend tick failed: {error}"));
    }
}

fn load_with_owner_revisions(
    registries: &Registries,
    state: &AppState,
    player_work_revision: Option<u64>,
    survival_revision: Option<u64>,
) -> AppState {
    let mut encoded = serde_json::to_value(SaveEnvelope::new(registries, state))
        .unwrap_or_else(|error| panic!("owner-revision fixture serialization failed: {error}"));
    if let Some(revision) = player_work_revision {
        encoded["state"]["systems"]["player_work"]["revision"] = serde_json::json!(revision);
    }
    if let Some(revision) = survival_revision {
        encoded["state"]["systems"]["survival"]["revision"] = serde_json::json!(revision);
    }
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("owner-revision fixture decode failed: {error}"));
    decoded
        .into_state(registries)
        .unwrap_or_else(|error| panic!("owner-revision fixture should load: {error}"))
}

fn direct_consumption_fixture(
    registries: &Registries,
    seed: u64,
) -> (
    AppState,
    crate::inventory::StockpileId,
    crate::inventory::MaterialLotId,
    crate::fluid::FluidStoreId,
) {
    let mut state = AppState::new(WorldSeed::new(seed));
    initialize_and_spend_reserves(registries, &mut state);
    let stockpile = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(10))
        .unwrap_or_else(|error| panic!("direct-consumption revision stockpile failed: {error}"));
    let food = deposit_lot_for_test(
        registries,
        &mut state,
        stockpile,
        CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD),
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("direct-consumption revision food failed: {error}"));
    let water = add_fluid_store_with_contents_for_fixture(
        registries,
        &mut state,
        Volume::from_microliters(10),
        FLUID_WATER,
        Volume::from_microliters(10),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("direct-consumption revision water failed: {error}"));
    (state, stockpile, food, water)
}

#[path = "consumption_tests/admission.rs"]
mod admission;

fn finish_direct_consumption(registries: &Registries, state: &mut AppState) -> u64 {
    let active = state
        .player_work()
        .active()
        .unwrap_or_else(|| panic!("direct-consumption test has no active player work"));
    let completes_at = match active {
        PlayerWork::Eating { work } => work.completes_at(),
        PlayerWork::Drinking { work } => work.completes_at(),
        other @ (PlayerWork::ManualProduction { .. }
        | PlayerWork::Mining { .. }
        | PlayerWork::ManualPower { .. }
        | PlayerWork::Prospecting { .. }
        | PlayerWork::EquipmentMaintenance { .. }
        | PlayerWork::StorageEnclosureDismantling { .. }) => {
            panic!("direct-consumption test has wrong active work: {other:?}")
        }
    };
    let started = state.tick().value();
    while state.tick() < completes_at {
        let _ = advance_tick(registries, state)
            .unwrap_or_else(|error| panic!("direct-consumption completion tick failed: {error}"));
    }
    assert_eq!(state.player_work().active(), None);
    completes_at.value() - started
}

#[path = "consumption_tests/death_attention.rs"]
mod death_attention;

fn start_attention_owning_craft(registries: &Registries, state: &mut AppState) -> PlayerWork {
    let source = add_solid_stockpile_for_test(state, Mass::from_milligrams(1_000_000))
        .unwrap_or_else(|error| panic!("attention craft source fixture failed: {error}"));
    let destination = add_solid_stockpile_for_test(state, Mass::from_milligrams(1_000_000))
        .unwrap_or_else(|error| panic!("attention craft destination fixture failed: {error}"));
    let lot = deposit_lot_for_test(
        registries,
        state,
        source,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(1_000_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("attention craft material fixture failed: {error}"));
    validate_start_manual_craft(
        registries,
        state,
        ManualCraftStartRequest::single(
            PROCESS_KNAP_STONE_TOOL,
            source,
            MaterialLotSelection::new(lot, Mass::from_milligrams(1_000_000)),
            destination,
        ),
    )
    .unwrap_or_else(|error| panic!("attention craft validation failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| panic!("attention craft commit failed: {error}"));
    state
        .player_work()
        .active()
        .unwrap_or_else(|| panic!("attention craft did not claim player work"))
}

#[path = "consumption_tests/commit_races.rs"]
mod commit_races;

#[path = "consumption_tests/persistence_absorption.rs"]
mod persistence_absorption;

#[path = "consumption_tests/persistence_validation.rs"]
mod persistence_validation;

#[path = "consumption_tests/custody_freshness.rs"]
mod custody_freshness;
