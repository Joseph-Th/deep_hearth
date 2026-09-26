//! Familiar selected-stack/vessel direct-consumption contracts.

use super::*;
use crate::content::{FLUID_WATER, FORM_FOOD, MATERIAL_BERRIES, build_registries};
use crate::core::quantity::{Mass, Temperature, Volume};
use crate::core::state::AppState;
use crate::fluid::add_fluid_store_with_contents_for_fixture;
use crate::inventory::{add_solid_stockpile_for_test, deposit_lot_for_test};
use crate::material::CommodityKey;
use crate::simulation::advance_tick;
use crate::survival::initialize_player_survival;

#[test]
fn selected_food_stack_default_use_fills_toward_full_without_manual_mass_entry() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("selected-stack survival setup failed: {error}"));
    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("selected-stack depletion tick failed: {error}"));
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(200_000))
        .unwrap_or_else(|error| panic!("selected-stack stockpile failed: {error}"));
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
        Mass::from_milligrams(100_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("selected-stack food failed: {error}"));
    let validated = validate_eat_lot_to_full(&registries, &state, lot)
        .unwrap_or_else(|error| panic!("selected-stack eating validation failed: {error}"))
        .unwrap_or_else(|| panic!("one depletion tick should require a meal"));
    let outcome = validated
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("selected-stack eating commit failed: {error}"));

    assert_eq!(
        outcome.total_mass(),
        registries
            .survival()
            .physiology()
            .direct_consumption()
            .minimum_meal_mass()
    );
}

#[test]
fn selected_water_store_default_use_fills_toward_full_without_manual_volume_entry() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("selected-vessel survival setup failed: {error}"));
    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("selected-vessel depletion tick failed: {error}"));
    let store = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        Volume::from_microliters(500_000),
        FLUID_WATER,
        Volume::from_microliters(250_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("selected-vessel water fixture failed: {error}"));
    let validated = validate_drink_store_to_full(&registries, &state, store)
        .unwrap_or_else(|error| panic!("selected-vessel drink validation failed: {error}"))
        .unwrap_or_else(|| panic!("one depletion tick should require a drink"));
    let outcome = validated
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("selected-vessel drink commit failed: {error}"));

    assert_eq!(
        outcome.volume(),
        registries
            .survival()
            .physiology()
            .direct_consumption()
            .minimum_drink_volume()
    );
}

#[test]
fn selected_stack_default_use_is_a_noop_when_already_full() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("selected-stack noop survival setup failed: {error}"));
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100_000))
        .unwrap_or_else(|error| panic!("selected-stack noop stockpile failed: {error}"));
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
        Mass::from_milligrams(100_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("selected-stack noop food failed: {error}"));
    let before = state.clone();

    assert!(
        validate_eat_lot_to_full(&registries, &state, lot)
            .unwrap_or_else(|error| panic!("selected-stack noop validation failed: {error}"))
            .is_none()
    );
    assert_eq!(state, before);
}

#[test]
fn selected_food_stack_default_use_preserves_canonical_temperature_rejection() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("hot-food survival setup failed: {error}"));
    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("hot-food depletion tick failed: {error}"));
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100_000))
        .unwrap_or_else(|error| panic!("hot-food stockpile failed: {error}"));
    let temperature = Temperature::from_millikelvin(340_000);
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
        Mass::from_milligrams(100_000),
        temperature,
    )
    .unwrap_or_else(|error| panic!("hot-food lot failed: {error}"));

    assert!(matches!(
        validate_eat_lot_to_full(&registries, &state, lot),
        Err(EatLotToTargetError::Eat(
            EatError::TemperatureOutsideConsumptionRange {
                lot: rejected,
                temperature: found,
                ..
            }
        )) if rejected == lot && found == temperature
    ));
}

#[test]
fn selected_water_store_default_use_preserves_canonical_temperature_rejection() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("hot-water survival setup failed: {error}"));
    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("hot-water depletion tick failed: {error}"));
    let temperature = Temperature::from_millikelvin(340_000);
    let store = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        Volume::from_microliters(500_000),
        FLUID_WATER,
        Volume::from_microliters(250_000),
        temperature,
    )
    .unwrap_or_else(|error| panic!("hot-water fixture failed: {error}"));

    assert!(matches!(
        validate_drink_store_to_full(&registries, &state, store),
        Err(DrinkStoreToTargetError::Drink(
            DrinkError::TemperatureOutsideConsumptionRange {
                store: rejected,
                temperature: found,
                ..
            }
        )) if rejected == store && found == temperature
    ));
}
