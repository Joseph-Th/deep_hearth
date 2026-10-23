//! Familiar selected-stack/vessel direct-consumption contracts.

use super::*;
use crate::content::{FLUID_WATER, FORM_FOOD, MATERIAL_BERRIES, MATERIAL_GRAIN, build_registries};
use crate::core::quantity::{Energy, Mass, Temperature, Volume};
use crate::core::state::AppState;
use crate::fluid::add_fluid_store_with_contents_for_fixture;
use crate::inventory::{
    add_solid_stockpile_for_test, deposit_lot_for_test, project_material_stacks,
};
use crate::logistics::{
    PlayerFluidStoreAccessError, PlayerStockpileAccessError, validate_initialize_player_logistics,
    validate_place_fluid_store, validate_place_ground_stockpile,
};
use crate::material::CommodityKey;
use crate::simulation::advance_tick;
use crate::spatial::VoxelCoord;
use crate::survival::{
    FoodCategory, NutritionReserves, Vitality, initialize_player_survival, player_record,
};

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
fn visible_stack_meal_composes_multiple_food_categories_without_lot_ids() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("stack meal survival setup failed: {error}"));
    let physiology = registries.survival().physiology();
    set_player_reserves(&mut state, Energy::ZERO, physiology.maximum_hydration());
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(500_000))
        .unwrap_or_else(|error| panic!("stack meal stockpile failed: {error}"));
    for material in [MATERIAL_BERRIES, MATERIAL_GRAIN] {
        deposit_lot_for_test(
            &registries,
            &mut state,
            source,
            CommodityKey::new(material, FORM_FOOD),
            Mass::from_milligrams(100_000),
            Temperature::from_millikelvin(293_150),
        )
        .unwrap_or_else(|error| panic!("stack meal food fixture failed: {error}"));
    }
    let stacks = project_material_stacks(&registries, &state, source)
        .unwrap_or_else(|error| panic!("stack meal projection failed: {error}"));
    assert_eq!(stacks.len(), 2);
    for stack in &stacks {
        assert!(matches!(
            assess_food_stack_freshness(&registries, &state, stack),
            Ok(FoodFreshness::Fresh { .. })
        ));
    }
    let portion_mass = Mass::from_milligrams(10_000);
    let portions = stacks
        .iter()
        .map(|stack| FoodStackPortion::new(stack, portion_mass))
        .collect::<Vec<_>>();
    let outcome = validate_eat_stack_portions(&registries, &state, &portions)
        .unwrap_or_else(|error| panic!("stack meal validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("stack meal commit failed: {error}"));
    assert_eq!(outcome.total_mass(), Mass::from_milligrams(20_000));
    assert_eq!(outcome.portions().len(), 2);
}

#[test]
fn repeated_visible_stack_portions_coalesce_without_overdrawing_one_physical_lot() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state).unwrap();
    set_player_reserves(
        &mut state,
        Energy::ZERO,
        registries.survival().physiology().maximum_hydration(),
    );
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(50_000)).unwrap();
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
        Mass::from_milligrams(50_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    let stack = project_material_stacks(&registries, &state, source)
        .unwrap()
        .remove(0);
    let before_rejection = state.clone();
    assert!(matches!(
        validate_eat_stack_portions(
            &registries,
            &state,
            &[
                FoodStackPortion::new(&stack, Mass::from_milligrams(30_000)),
                FoodStackPortion::new(&stack, Mass::from_milligrams(30_000)),
            ],
        ),
        Err(EatStackPortionsError::Eat(EatError::InsufficientLotMass {
            lot: rejected,
            available,
            requested,
        })) if rejected == lot
            && available == Mass::from_milligrams(50_000)
            && requested == Mass::from_milligrams(60_000)
    ));
    assert_eq!(state, before_rejection);

    let outcome = validate_eat_stack_portions(
        &registries,
        &state,
        &[
            FoodStackPortion::new(&stack, Mass::from_milligrams(15_000)),
            FoodStackPortion::new(&stack, Mass::from_milligrams(20_000)),
        ],
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    assert_eq!(outcome.total_mass(), Mass::from_milligrams(35_000));
    assert_eq!(outcome.portions().len(), 1);
    assert_eq!(outcome.portions()[0].lot(), lot);
    assert_eq!(
        state.inventory().get_lot(lot).map(|lot| lot.mass()),
        Some(Mass::from_milligrams(15_000))
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(source)
            .unwrap()
            .stored_mass(),
        Mass::from_milligrams(15_000)
    );
}

#[test]
fn full_metabolic_reserves_do_not_block_a_needed_dietary_serving() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state).unwrap();
    let physiology = registries.survival().physiology();
    let revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        revision,
        revision + 1,
        player_record(
            physiology.maximum_metabolic_energy(),
            physiology.maximum_hydration(),
            Vitality::MAXIMUM,
            NutritionReserves::from_parts_per_million(1_000_000, 500_000, 1_000_000),
            0,
        ),
    );
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(700_000)).unwrap();
    let berries = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
        Mass::from_milligrams(500_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    let grain = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD),
        Mass::from_milligrams(100_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    let stacks = project_material_stacks(&registries, &state, source).unwrap();
    let berry_stack = stacks
        .iter()
        .find(|stack| stack.commodity().material() == MATERIAL_BERRIES)
        .unwrap();
    let grain_stack = stacks
        .iter()
        .find(|stack| stack.commodity().material() == MATERIAL_GRAIN)
        .unwrap();
    assert!(
        validate_eat_lot_to_full(&registries, &state, grain)
            .unwrap()
            .is_none()
    );
    assert!(
        validate_eat_stack_to_full(&registries, &state, grain_stack)
            .unwrap()
            .is_none()
    );
    assert!(
        validate_eat_lot_to_metabolic_target(
            &registries,
            &state,
            berries,
            physiology.maximum_metabolic_energy(),
        )
        .unwrap()
        .is_none()
    );

    let mut lot_state = state.clone();
    let stack_meal = validate_eat_stack_to_full(&registries, &state, berry_stack)
        .unwrap()
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let lot_meal = validate_eat_lot_to_full(&registries, &lot_state, berries)
        .unwrap()
        .unwrap()
        .commit(&mut lot_state)
        .unwrap();
    assert_eq!(stack_meal, lot_meal);
    assert_eq!(state, lot_state);
    let full_reserve_mass = physiology.nutrition().full_reserve_food_mass().milligrams();
    let expected_refill = Mass::from_milligrams((full_reserve_mass + 1) / 2);
    assert_eq!(stack_meal.total_mass(), expected_refill);
    assert_eq!(
        state.inventory().get_lot(berries).map(|lot| lot.mass()),
        Some(
            Mass::from_milligrams(500_000)
                .checked_sub(expected_refill)
                .unwrap()
        )
    );
    assert_eq!(
        stack_meal.nutrition_offered().get(FoodCategory::Fruit),
        500_000
    );
    while state.tick() < stack_meal.completes_at() {
        let _ = advance_tick(&registries, &mut state).unwrap();
        let _ = advance_tick(&registries, &mut lot_state).unwrap();
    }
    assert_eq!(state, lot_state);
    assert!(
        state
            .survival()
            .player()
            .unwrap()
            .nutrition()
            .get(FoodCategory::Fruit)
            > 500_000
    );
}

#[test]
fn projected_food_stack_use_consumes_only_the_selected_physical_cohort() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("projected-stack survival setup failed: {error}"));
    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("projected-stack depletion tick failed: {error}"));
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2_000_000))
        .unwrap_or_else(|error| panic!("projected-stack stockpile failed: {error}"));
    let commodity = CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD);
    deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        commodity,
        Mass::from_milligrams(500_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("projected-stack first cohort failed: {error}"));
    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("projected-stack age separation tick failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        commodity,
        Mass::from_milligrams(500_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("projected-stack second cohort failed: {error}"));
    let stacks = project_material_stacks(&registries, &state, source)
        .unwrap_or_else(|error| panic!("projected-stack projection failed: {error}"));
    assert_eq!(stacks.len(), 2);
    let fresh = stacks
        .iter()
        .min_by_key(|stack| stack.ambient_storage_age_parts())
        .unwrap_or_else(|| panic!("projected-stack fresh cohort disappeared"));
    let old = stacks
        .iter()
        .max_by_key(|stack| stack.ambient_storage_age_parts())
        .unwrap_or_else(|| panic!("projected-stack old cohort disappeared"));
    let old_mass = old.mass();
    let fresh_before = fresh.mass();

    let validated = validate_eat_stack_to_full(&registries, &state, fresh)
        .unwrap_or_else(|error| panic!("projected-stack eating failed: {error}"))
        .unwrap_or_else(|| panic!("projected-stack depleted player unexpectedly needs no meal"));
    let eaten = validated
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("projected-stack eating commit failed: {error}"))
        .total_mass();
    let remaining = project_material_stacks(&registries, &state, source)
        .unwrap_or_else(|error| panic!("projected-stack remaining projection failed: {error}"));
    let remaining_old = remaining
        .iter()
        .max_by_key(|stack| stack.ambient_storage_age_parts())
        .unwrap_or_else(|| panic!("projected-stack old cohort vanished"));
    let remaining_fresh = remaining
        .iter()
        .min_by_key(|stack| stack.ambient_storage_age_parts())
        .unwrap_or_else(|| panic!("projected-stack fresh cohort vanished"));
    assert_eq!(remaining_old.mass(), old_mass);
    assert_eq!(
        remaining_fresh.mass(),
        fresh_before
            .checked_sub(eaten)
            .unwrap_or_else(|| panic!("projected-stack meal exceeded fresh cohort"))
    );
}

#[test]
fn projected_food_stack_use_rejects_stale_inventory_revision_without_mutation() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("stale projected-stack survival setup failed: {error}"));
    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("stale projected-stack depletion tick failed: {error}"));
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000))
        .unwrap_or_else(|error| panic!("stale projected-stack stockpile failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
        Mass::from_milligrams(500_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("stale projected-stack food failed: {error}"));
    let stack = project_material_stacks(&registries, &state, source)
        .unwrap_or_else(|error| panic!("stale projected-stack projection failed: {error}"))
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("stale projected-stack projection was empty"));

    let _unrelated = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1))
        .unwrap_or_else(|error| panic!("stale projected-stack revision mutation failed: {error}"));
    let before = state.clone();

    assert!(matches!(
        assess_food_stack_freshness(&registries, &state, &stack),
        Err(FoodStackFreshnessError::Stack(
            crate::inventory::MaterialStackProjectionError::StaleInventoryRevision { .. }
        ))
    ));

    assert!(matches!(
        validate_eat_stack_to_full(&registries, &state, &stack),
        Err(EatStackToTargetError::Stack(
            crate::inventory::MaterialStackProjectionError::StaleInventoryRevision { .. }
        ))
    ));
    assert_eq!(state, before);
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

#[test]
fn selected_food_stack_rejects_remote_source_before_disclosing_stack_mass() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("remote selected-stack survival setup failed: {error}"));
    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("remote selected-stack depletion tick failed: {error}"));
    let player_position = VoxelCoord::new(0, 0, 0);
    validate_initialize_player_logistics(&state, player_position, Mass::from_milligrams(1))
        .unwrap_or_else(|error| panic!("remote selected-stack logistics setup failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("remote selected-stack logistics commit failed: {error}"));
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1))
        .unwrap_or_else(|error| panic!("remote selected-stack stockpile failed: {error}"));
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
        Mass::from_milligrams(1),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("remote selected-stack food failed: {error}"));
    let source_position = VoxelCoord::new(1, 0, 0);
    validate_place_ground_stockpile(&state, source, source_position)
        .unwrap_or_else(|error| panic!("remote selected-stack placement failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("remote selected-stack placement commit failed: {error}"));
    let before = state.clone();

    assert_eq!(
        validate_eat_lot_to_full(&registries, &state, lot).err(),
        Some(EatLotToTargetError::Eat(EatError::Access(
            PlayerStockpileAccessError::RemoteKnownStockpile {
                stockpile: source,
                stockpile_position: source_position,
                player_position,
            }
        )))
    );
    assert_eq!(state, before);
}

#[test]
fn selected_water_store_rejects_remote_source_before_disclosing_volume() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("remote selected-vessel survival setup failed: {error}"));
    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("remote selected-vessel depletion tick failed: {error}"));
    let player_position = VoxelCoord::new(0, 0, 0);
    validate_initialize_player_logistics(&state, player_position, Mass::from_milligrams(1))
        .unwrap_or_else(|error| panic!("remote selected-vessel logistics setup failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("remote selected-vessel logistics commit failed: {error}"));
    let store = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        Volume::from_microliters(1),
        FLUID_WATER,
        Volume::from_microliters(1),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("remote selected-vessel water failed: {error}"));
    let store_position = VoxelCoord::new(1, 0, 0);
    validate_place_fluid_store(&state, store, store_position)
        .unwrap_or_else(|error| panic!("remote selected-vessel placement failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("remote selected-vessel placement commit failed: {error}"));
    let before = state.clone();

    assert_eq!(
        validate_drink_store_to_full(&registries, &state, store).err(),
        Some(DrinkStoreToTargetError::Drink(DrinkError::Access(
            PlayerFluidStoreAccessError::RemoteKnownFluidStore {
                store,
                store_position,
                player_position,
            }
        )))
    );
    assert_eq!(state, before);
}
