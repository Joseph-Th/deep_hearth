//! Familiar selected-stack/vessel direct-consumption contracts.

use super::*;
use crate::content::{FLUID_WATER, FORM_FOOD, MATERIAL_BERRIES, build_registries};
use crate::core::quantity::{Energy, Mass, Temperature, Volume};
use crate::core::state::AppState;
use crate::fluid::add_fluid_store_with_contents_for_fixture;
use crate::inventory::{add_solid_stockpile_for_test, deposit_lot_for_test};
use crate::material::CommodityKey;
use crate::simulation::advance_tick;
use crate::survival::{NutritionReserves, Vitality, initialize_player_survival, player_record};

fn set_player_reserves(state: &mut AppState, metabolic_energy: Energy, hydration: Volume) {
    let expected_revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        expected_revision,
        expected_revision + 1,
        player_record(
            metabolic_energy,
            hydration,
            Vitality::MAXIMUM,
            NutritionReserves::FULL,
            0,
        ),
    );
}

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
fn selected_food_stack_default_use_makes_partial_progress_when_full_is_unreachable() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("partial-stack survival setup failed: {error}"));
    let physiology = registries.survival().physiology();
    set_player_reserves(&mut state, Energy::ZERO, physiology.maximum_hydration());
    let available = Mass::from_milligrams(500_000);
    let source = add_solid_stockpile_for_test(&mut state, available)
        .unwrap_or_else(|error| panic!("partial-stack stockpile failed: {error}"));
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
        available,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("partial-stack food failed: {error}"));

    assert!(matches!(
        validate_eat_lot_to_metabolic_target(
            &registries,
            &state,
            lot,
            physiology.maximum_metabolic_energy(),
        ),
        Err(EatLotToTargetError::Projection(
            MealMetabolicProjectionError::TargetUnreachableWithinIntakeLimit { .. }
        ))
    ));
    let validated = validate_eat_lot_to_full(&registries, &state, lot)
        .unwrap_or_else(|error| panic!("partial-stack default eating failed: {error}"))
        .unwrap_or_else(|| panic!("empty reserves require a meal"));
    let outcome = validated
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("partial-stack eating commit failed: {error}"));

    assert_eq!(outcome.total_mass(), available);
}

#[test]
fn selected_food_stack_default_use_consumes_available_legal_portion_when_refill_needs_more() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("short-stack survival setup failed: {error}"));
    let physiology = registries.survival().physiology();
    let metabolic_energy = physiology
        .maximum_metabolic_energy()
        .checked_sub(Energy::from_nanojoules(500_000_000_000_000))
        .unwrap_or_else(|| panic!("short-stack reserve fixture underflowed"));
    set_player_reserves(&mut state, metabolic_energy, physiology.maximum_hydration());
    let available = Mass::from_milligrams(100_000);
    let source = add_solid_stockpile_for_test(&mut state, available)
        .unwrap_or_else(|error| panic!("short-stack stockpile failed: {error}"));
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
        available,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("short-stack food failed: {error}"));

    assert!(matches!(
        validate_eat_lot_to_metabolic_target(
            &registries,
            &state,
            lot,
            physiology.maximum_metabolic_energy(),
        ),
        Err(EatLotToTargetError::InsufficientLotMass {
            available: found,
            ..
        }) if found == available
    ));
    let outcome = validate_eat_lot_to_full(&registries, &state, lot)
        .unwrap_or_else(|error| panic!("short-stack default eating failed: {error}"))
        .unwrap_or_else(|| panic!("partial reserve requires a meal"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("short-stack eating commit failed: {error}"));

    assert_eq!(outcome.total_mass(), available);
}

#[test]
fn selected_water_store_default_use_makes_partial_progress_when_full_is_unreachable() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("partial-vessel survival setup failed: {error}"));
    let physiology = registries.survival().physiology();
    set_player_reserves(
        &mut state,
        physiology.maximum_metabolic_energy(),
        Volume::ZERO,
    );
    let available = Volume::from_microliters(500_000);
    let store = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        available,
        FLUID_WATER,
        available,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("partial-vessel water fixture failed: {error}"));

    assert!(matches!(
        validate_drink_store_to_hydration_target(
            &registries,
            &state,
            store,
            physiology.maximum_hydration(),
        ),
        Err(DrinkStoreToTargetError::Projection(
            DrinkHydrationProjectionError::TargetUnreachableWithinIntakeLimit { .. }
        ))
    ));
    let validated = validate_drink_store_to_full(&registries, &state, store)
        .unwrap_or_else(|error| panic!("partial-vessel default drinking failed: {error}"))
        .unwrap_or_else(|| panic!("empty hydration requires a drink"));
    let outcome = validated
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("partial-vessel drinking commit failed: {error}"));

    assert_eq!(outcome.volume(), available);
}

#[test]
fn selected_water_store_default_use_consumes_one_maximum_serving_when_refill_needs_more() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("short-vessel survival setup failed: {error}"));
    let physiology = registries.survival().physiology();
    let hydration = Volume::from_microliters(3_000_000);
    set_player_reserves(&mut state, physiology.maximum_metabolic_energy(), hydration);
    let available = physiology.direct_consumption().maximum_drink_volume();
    let store = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        available,
        FLUID_WATER,
        available,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("short-vessel water fixture failed: {error}"));

    assert!(matches!(
        validate_drink_store_to_hydration_target(
            &registries,
            &state,
            store,
            physiology.maximum_hydration(),
        ),
        Err(DrinkStoreToTargetError::Projection(
            DrinkHydrationProjectionError::TargetUnreachableWithinIntakeLimit {
                maximum_drink_volume,
            }
        )) if maximum_drink_volume == available
    ));
    let outcome = validate_drink_store_to_full(&registries, &state, store)
        .unwrap_or_else(|error| panic!("short-vessel default drinking failed: {error}"))
        .unwrap_or_else(|| panic!("partial hydration requires a drink"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("short-vessel drinking commit failed: {error}"));

    assert_eq!(outcome.volume(), available);
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
