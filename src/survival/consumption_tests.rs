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
use crate::core::time::{SimulationTick, TickSpan};
use crate::crafting::{ManualCraftStartRequest, validate_start_manual_craft};
use crate::fluid::{add_fluid_store_with_contents_for_fixture, calculate_fluid_volume_accounting};
use crate::inventory::{
    MaterialLotSelection, StockpileStorageProfile, add_solid_stockpile_for_test, add_stockpile,
    deposit_lot_for_test, validate_build_storage_enclosure, validate_material_relocation_for_test,
};
use crate::labor::{PlayerWork, PlayerWorkValidationError};
use crate::logistics::{
    PlayerFluidStoreAccessError, PlayerStockpileAccessError, validate_initialize_player_logistics,
    validate_place_fluid_store, validate_place_ground_stockpile,
};
use crate::material::CommodityKey;
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
use crate::registry::Registries;
use crate::simulation::advance_tick;
use crate::spatial::VoxelCoord;
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

#[test]
fn drinking_rejects_known_remote_fluid_store() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_and_spend_reserves(&registries, &mut state);
    let player_position = VoxelCoord::new(0, 0, 0);
    validate_initialize_player_logistics(&state, player_position, Mass::from_milligrams(1))
        .unwrap_or_else(|error| panic!("remote drinking logistics setup failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("remote drinking logistics commit failed: {error}"));
    let volume = minimum_drink_volume(&registries);
    let water = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        volume,
        FLUID_WATER,
        volume,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("remote drinking water fixture failed: {error}"));
    let store_position = VoxelCoord::new(1, 0, 0);
    validate_place_fluid_store(&state, water, store_position)
        .unwrap_or_else(|error| panic!("remote drinking placement failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("remote drinking placement commit failed: {error}"));
    let before = state.clone();

    assert_eq!(
        validate_drink(&registries, &state, water, volume).err(),
        Some(DrinkError::Access(
            PlayerFluidStoreAccessError::RemoteKnownFluidStore {
                store: water,
                store_position,
                player_position,
            }
        ))
    );
    assert_eq!(state, before);
}

#[test]
fn drinking_token_rejects_fluid_location_added_after_validation() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_and_spend_reserves(&registries, &mut state);
    let volume = minimum_drink_volume(&registries);
    let water = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        volume,
        FLUID_WATER,
        volume,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("stale-location drinking water fixture failed: {error}"));
    let validated = validate_drink(&registries, &state, water, volume)
        .unwrap_or_else(|error| panic!("stale-location drinking validation failed: {error}"));
    let expected = state.logistics().revision();
    validate_place_fluid_store(&state, water, VoxelCoord::new(5, 0, 0))
        .unwrap_or_else(|error| panic!("stale-location drinking placement failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("stale-location drinking placement commit failed: {error}"));
    let actual = state.logistics().revision();
    let before = state.clone();

    assert_eq!(
        validated.commit(&mut state),
        Err(DrinkCommitError::StaleLogisticsRevision { expected, actual })
    );
    assert_eq!(state, before);
}

#[test]
fn eating_rejects_known_remote_ground_food_source() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_and_spend_reserves(&registries, &mut state);
    let player_position = VoxelCoord::new(0, 0, 0);
    validate_initialize_player_logistics(&state, player_position, Mass::from_milligrams(1))
        .unwrap_or_else(|error| panic!("remote eating logistics setup failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("remote eating logistics commit failed: {error}"));
    let meal_mass = minimum_meal_mass(&registries);
    let source = add_solid_stockpile_for_test(&mut state, meal_mass)
        .unwrap_or_else(|error| panic!("remote eating source failed: {error}"));
    let food = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD),
        meal_mass,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("remote eating food failed: {error}"));
    let source_position = VoxelCoord::new(1, 0, 0);
    validate_place_ground_stockpile(&state, source, source_position)
        .unwrap_or_else(|error| panic!("remote eating placement failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("remote eating placement commit failed: {error}"));
    let before = state.clone();

    assert_eq!(
        validate_eat(
            &registries,
            &state,
            source,
            &[MaterialLotSelection::new(food, meal_mass)],
        )
        .err(),
        Some(EatError::Access(
            PlayerStockpileAccessError::RemoteKnownStockpile {
                stockpile: source,
                stockpile_position: source_position,
                player_position,
            }
        ))
    );
    assert_eq!(state, before);
}

#[test]
fn eating_token_rejects_ground_location_added_after_validation() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_and_spend_reserves(&registries, &mut state);
    let meal_mass = minimum_meal_mass(&registries);
    let source = add_solid_stockpile_for_test(&mut state, meal_mass)
        .unwrap_or_else(|error| panic!("stale-location eating source failed: {error}"));
    let food = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD),
        meal_mass,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("stale-location eating food failed: {error}"));
    let validated = validate_eat(
        &registries,
        &state,
        source,
        &[MaterialLotSelection::new(food, meal_mass)],
    )
    .unwrap_or_else(|error| panic!("stale-location eating validation failed: {error}"));
    validate_place_ground_stockpile(&state, source, VoxelCoord::new(5, 0, 0))
        .unwrap_or_else(|error| panic!("stale-location eating placement failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("stale-location eating placement commit failed: {error}"));
    let before = state.clone();

    assert_eq!(
        validated.commit(&mut state),
        Err(EatCommitError::StaleLogisticsRevision {
            expected: 0,
            actual: 1,
        })
    );
    assert_eq!(state, before);
}

fn minimum_drink_volume(registries: &Registries) -> Volume {
    registries
        .survival()
        .physiology()
        .direct_consumption()
        .minimum_drink_volume()
}

fn minimum_meal_mass(registries: &Registries) -> Mass {
    registries
        .survival()
        .physiology()
        .direct_consumption()
        .minimum_meal_mass()
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
) -> (
    AppState,
    crate::inventory::StockpileId,
    crate::inventory::MaterialLotId,
    crate::fluid::FluidStoreId,
) {
    let mut state = AppState::new();
    initialize_and_spend_reserves(registries, &mut state);
    let meal_mass = minimum_meal_mass(registries);
    let stockpile = add_solid_stockpile_for_test(&mut state, meal_mass)
        .unwrap_or_else(|error| panic!("direct-consumption revision stockpile failed: {error}"));
    let food = deposit_lot_for_test(
        registries,
        &mut state,
        stockpile,
        CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD),
        meal_mass,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("direct-consumption revision food failed: {error}"));
    let water = add_fluid_store_with_contents_for_fixture(
        registries,
        &mut state,
        minimum_drink_volume(registries),
        FLUID_WATER,
        minimum_drink_volume(registries),
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

#[path = "consumption_tests/drink_projection.rs"]
mod drink_projection;

#[path = "consumption_tests/meal_projection.rs"]
mod meal_projection;

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
