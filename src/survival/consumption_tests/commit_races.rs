//! Eating and drinking admission, capacity, and stale-commit contracts.

use super::*;

#[test]
fn eating_and_drinking_reject_active_player_work_without_mutation() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("attention survival initialization failed: {error}"));
    let food_source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1))
        .unwrap_or_else(|error| panic!("attention food stockpile failed: {error}"));
    let food = deposit_lot_for_test(
        &registries,
        &mut state,
        food_source,
        CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD),
        Mass::from_milligrams(1),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("attention food lot failed: {error}"));
    let drink_volume = minimum_drink_volume(&registries);
    let water = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        drink_volume,
        FLUID_WATER,
        drink_volume,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("attention water fixture failed: {error}"));
    let active = start_attention_owning_craft(&registries, &mut state);
    let before = state.clone();

    assert_eq!(
        validate_eat(
            &registries,
            &state,
            food_source,
            &[MaterialLotSelection::new(food, Mass::from_milligrams(1))],
        )
        .err(),
        Some(EatError::PlayerBusy {
            active: Box::new(active),
        })
    );
    assert_eq!(
        validate_drink(&registries, &state, water, drink_volume).err(),
        Some(DrinkError::PlayerBusy {
            active: Box::new(active),
        })
    );
    assert_eq!(state, before);
}

#[test]
fn eating_with_any_reserve_room_consumes_the_exact_selected_portion() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("partial-reserve survival initialization failed: {error}"));
    let physiology = registries.survival().physiology();
    let meal_mass = minimum_meal_mass(&registries);
    let energy_before = physiology
        .maximum_metabolic_energy()
        .checked_sub(Energy::from_nanojoules(1))
        .unwrap_or_else(|| panic!("partial-reserve energy fixture underflowed"));
    let expected_revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        expected_revision,
        expected_revision + 1,
        player_record(
            energy_before,
            physiology.maximum_hydration(),
            Vitality::MAXIMUM,
            NutritionReserves::from_parts_per_million(
                NUTRITION_PARTS_PER_MILLION,
                NUTRITION_PARTS_PER_MILLION,
                NUTRITION_PARTS_PER_MILLION,
            ),
            0,
        ),
    );
    let stockpile = add_solid_stockpile_for_test(&mut state, meal_mass)
        .unwrap_or_else(|error| panic!("partial-reserve food stockpile failed: {error}"));
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD),
        meal_mass,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("partial-reserve food lot failed: {error}"));

    let outcome = validate_eat(
        &registries,
        &state,
        stockpile,
        &[MaterialLotSelection::new(lot, meal_mass)],
    )
    .unwrap_or_else(|error| panic!("partial-reserve eating validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("partial-reserve eating commit failed: {error}"));

    assert_eq!(outcome.total_mass(), meal_mass);
    assert_eq!(
        outcome.energy_offered(),
        registries
            .survival()
            .get_food(CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD))
            .unwrap_or_else(|| panic!("grain food definition disappeared"))
            .dietary_energy_for_mass(meal_mass)
    );
    assert_eq!(outcome.hydration_offered(), AggregateVolume::ZERO);
    assert!(outcome.nutrition_offered().get(FoodCategory::Grain) > 0);
    assert_eq!(outcome.nutrition_offered().get(FoodCategory::Fruit), 0);
    assert_eq!(outcome.nutrition_offered().get(FoodCategory::Protein), 0);
    assert_eq!(state.inventory().get_lot(lot), None);
    assert_eq!(
        assess_survival(&registries, &state)
            .unwrap_or_else(|| panic!("partial-reserve survival state disappeared"))
            .metabolic_energy(),
        energy_before
    );
    assert_eq!(finish_direct_consumption(&registries, &mut state), 1);
    assert_eq!(
        assess_survival(&registries, &state)
            .unwrap_or_else(|| panic!("partial-reserve survival state disappeared after eating"))
            .metabolic_energy(),
        energy_before
            .checked_sub(physiology.basal_energy_cost_per_tick())
            .and_then(|value| value.checked_add(outcome.energy_offered()))
            .unwrap_or_else(|| panic!("partial-reserve expected energy underflowed"))
            .min(physiology.maximum_metabolic_energy())
    );
}

#[test]
fn meal_energy_first_covers_same_tick_metabolic_shortfall() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("meal-shortfall survival setup failed: {error}"));
    let physiology = registries.survival().physiology();
    let meal_mass = minimum_meal_mass(&registries);
    let energy_before = Energy::from_nanojoules(320_000_000_000);
    let expected_revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        expected_revision,
        expected_revision + 1,
        player_record(
            energy_before,
            physiology.maximum_hydration(),
            Vitality::MAXIMUM,
            NutritionReserves::from_parts_per_million(
                NUTRITION_PARTS_PER_MILLION,
                NUTRITION_PARTS_PER_MILLION,
                NUTRITION_PARTS_PER_MILLION,
            ),
            0,
        ),
    );
    let stockpile = add_solid_stockpile_for_test(&mut state, meal_mass)
        .unwrap_or_else(|error| panic!("meal-shortfall stockpile failed: {error}"));
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD),
        meal_mass,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("meal-shortfall food lot failed: {error}"));
    let outcome = validate_eat(
        &registries,
        &state,
        stockpile,
        &[MaterialLotSelection::new(lot, meal_mass)],
    )
    .unwrap_or_else(|error| panic!("meal-shortfall validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("meal-shortfall commit failed: {error}"));
    assert_eq!(
        outcome.energy_offered(),
        registries
            .survival()
            .get_food(CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD))
            .unwrap_or_else(|| panic!("grain food definition disappeared"))
            .dietary_energy_for_mass(meal_mass)
    );
    assert_eq!(finish_direct_consumption(&registries, &mut state), 1);
    let shortfall = physiology
        .basal_energy_cost_per_tick()
        .checked_sub(energy_before)
        .unwrap_or(Energy::ZERO);
    let expected_after = outcome
        .energy_offered()
        .checked_sub(shortfall)
        .unwrap_or(Energy::ZERO);
    assert_eq!(
        assess_survival(&registries, &state)
            .unwrap_or_else(|| panic!("meal-shortfall player disappeared"))
            .metabolic_energy(),
        expected_after
    );
}

#[test]
fn eating_rejects_over_capacity_hydration_without_normalizing_or_consuming_food() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state).unwrap_or_else(|error| {
        panic!("invalid-hydration survival initialization failed: {error}")
    });
    let physiology = registries.survival().physiology();
    let invalid_hydration = physiology
        .maximum_hydration()
        .checked_add(Volume::from_microliters(1))
        .unwrap_or_else(|| panic!("invalid-hydration fixture overflowed"));
    let expected_revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        expected_revision,
        expected_revision + 1,
        player_record(
            physiology.maximum_metabolic_energy(),
            invalid_hydration,
            Vitality::MAXIMUM,
            NutritionReserves::from_parts_per_million(0, 0, 0),
            0,
        ),
    );
    let stockpile = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1))
        .unwrap_or_else(|error| panic!("invalid-hydration food stockpile failed: {error}"));
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD),
        Mass::from_milligrams(1),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("invalid-hydration food lot failed: {error}"));
    let before = state.clone();

    assert_eq!(
        validate_eat(
            &registries,
            &state,
            stockpile,
            &[MaterialLotSelection::new(lot, Mass::from_milligrams(1))],
        )
        .err(),
        Some(EatError::HydrationOverflow)
    );
    assert_eq!(state, before);
}

#[test]
fn validated_drink_rejects_player_work_started_before_commit_without_mutation() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_and_spend_reserves(&registries, &mut state);
    let drink_volume = minimum_drink_volume(&registries);
    let water = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        drink_volume,
        FLUID_WATER,
        drink_volume,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("stale-attention water fixture failed: {error}"));
    let drink = validate_drink(&registries, &state, water, drink_volume)
        .unwrap_or_else(|error| panic!("stale-attention drink validation failed: {error}"));

    start_attention_owning_craft(&registries, &mut state);
    let before = state.clone();
    assert_eq!(
        drink.commit(&mut state),
        Err(DrinkCommitError::StalePlayerWorkRevision {
            expected: 0,
            actual: 1,
        })
    );
    assert_eq!(state, before);
}

#[test]
fn validated_eat_rejects_survival_change_before_commit_without_mutation() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_and_spend_reserves(&registries, &mut state);
    let meal_mass = minimum_meal_mass(&registries);
    let stockpile = add_solid_stockpile_for_test(&mut state, meal_mass)
        .unwrap_or_else(|error| panic!("stale-survival meal stockpile failed: {error}"));
    let food = deposit_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD),
        meal_mass,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("stale-survival meal fixture failed: {error}"));
    let token = validate_eat(
        &registries,
        &state,
        stockpile,
        &[MaterialLotSelection::new(food, meal_mass)],
    )
    .unwrap_or_else(|error| panic!("stale-survival meal validation failed: {error}"));
    let expected = state.survival().revision();

    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("stale-survival meal setup tick failed: {error}"));
    let actual = state.survival().revision();
    let before_commit = state.clone();

    assert_eq!(
        token.commit(&mut state),
        Err(EatCommitError::StaleSurvivalRevision { expected, actual })
    );
    assert_eq!(state, before_commit);
}

#[test]
fn validated_eat_rejects_inventory_change_before_commit_without_mutation() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_and_spend_reserves(&registries, &mut state);
    let meal_mass = minimum_meal_mass(&registries);
    let stockpile = add_solid_stockpile_for_test(&mut state, meal_mass)
        .unwrap_or_else(|error| panic!("stale-inventory meal stockpile failed: {error}"));
    let food = deposit_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD),
        meal_mass,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("stale-inventory meal fixture failed: {error}"));
    let token = validate_eat(
        &registries,
        &state,
        stockpile,
        &[MaterialLotSelection::new(food, meal_mass)],
    )
    .unwrap_or_else(|error| panic!("stale-inventory meal validation failed: {error}"));
    let expected = state.inventory().revision();

    add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1))
        .unwrap_or_else(|error| panic!("stale-inventory intervening mutation failed: {error}"));
    let actual = state.inventory().revision();
    let before_commit = state.clone();

    assert_eq!(
        token.commit(&mut state),
        Err(EatCommitError::StaleInventoryRevision { expected, actual })
    );
    assert_eq!(state, before_commit);
}

#[test]
fn validated_drink_rejects_survival_change_before_commit_without_mutation() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_and_spend_reserves(&registries, &mut state);
    let drink_volume = minimum_drink_volume(&registries);
    let water = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        drink_volume,
        FLUID_WATER,
        drink_volume,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("stale-survival drink fixture failed: {error}"));
    let token = validate_drink(&registries, &state, water, drink_volume)
        .unwrap_or_else(|error| panic!("stale-survival drink validation failed: {error}"));
    let expected = state.survival().revision();

    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("stale-survival drink setup tick failed: {error}"));
    let actual = state.survival().revision();
    let before_commit = state.clone();

    assert_eq!(
        token.commit(&mut state),
        Err(DrinkCommitError::StaleSurvivalRevision { expected, actual })
    );
    assert_eq!(state, before_commit);
}

#[test]
fn validated_drink_rejects_fluid_change_before_commit_without_mutation() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_and_spend_reserves(&registries, &mut state);
    let drink_volume = minimum_drink_volume(&registries);
    let water = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        drink_volume,
        FLUID_WATER,
        drink_volume,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("stale-fluid drink fixture failed: {error}"));
    let token = validate_drink(&registries, &state, water, drink_volume)
        .unwrap_or_else(|error| panic!("stale-fluid drink validation failed: {error}"));
    let expected = state.fluid().revision();

    add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        Volume::from_microliters(1),
        FLUID_WATER,
        Volume::from_microliters(1),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("stale-fluid intervening mutation failed: {error}"));
    let actual = state.fluid().revision();
    let before_commit = state.clone();

    assert_eq!(
        token.commit(&mut state),
        Err(DrinkCommitError::StaleFluidRevision { expected, actual })
    );
    assert_eq!(state, before_commit);
}
