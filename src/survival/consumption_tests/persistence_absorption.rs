//! Trusted-load replay, multi-tick absorption, and reserve-timing contracts.

use super::*;

#[test]
fn trusted_load_replays_direct_consumption_attention_durations() {
    let registries = build_registries();

    let mut eating = AppState::new();
    initialize_and_spend_reserves(&registries, &mut eating);
    let meal_mass = minimum_meal_mass(&registries);
    let stockpile = add_solid_stockpile_for_test(&mut eating, meal_mass)
        .unwrap_or_else(|error| panic!("eating-duration stockpile failed: {error}"));
    let food = deposit_lot_for_test(
        &registries,
        &mut eating,
        stockpile,
        CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD),
        meal_mass,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("eating-duration food fixture failed: {error}"));
    let _eating_outcome = validate_eat(
        &registries,
        &eating,
        stockpile,
        &[MaterialLotSelection::new(food, meal_mass)],
    )
    .unwrap_or_else(|error| panic!("eating-duration validation failed: {error}"))
    .commit(&mut eating)
    .unwrap_or_else(|error| panic!("eating-duration commit failed: {error}"));
    let mut tampered = serde_json::to_value(SaveEnvelope::new(&registries, &eating))
        .unwrap_or_else(|error| panic!("eating-duration serialization failed: {error}"));
    let completes_at =
        tampered["state"]["systems"]["player_work"]["active"]["Eating"]["work"]["completes_at"]
            .as_u64()
            .unwrap_or_else(|| panic!("eating-duration completion tick was not serialized as u64"));
    tampered["state"]["systems"]["player_work"]["active"]["Eating"]["work"]["completes_at"] =
        serde_json::json!(completes_at + 1);
    let tampered: LoadedSaveEnvelope = serde_json::from_value(tampered)
        .unwrap_or_else(|error| panic!("eating-duration tamper decode failed: {error}"));
    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::PlayerWork(
            PlayerWorkValidationError::EatingDurationMismatch
        )))
    );

    let mut tampered = serde_json::to_value(SaveEnvelope::new(&registries, &eating))
        .unwrap_or_else(|error| panic!("eating-mass serialization failed: {error}"));
    tampered["state"]["systems"]["player_work"]["active"]["Eating"]["work"]["mass"] =
        serde_json::json!(0_u64);
    let tampered: LoadedSaveEnvelope = serde_json::from_value(tampered)
        .unwrap_or_else(|error| panic!("eating-mass tamper decode failed: {error}"));
    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::PlayerWork(
            PlayerWorkValidationError::EatingMassInvalid { mass: Mass::ZERO }
        )))
    );

    let mut drinking = AppState::new();
    initialize_and_spend_reserves(&registries, &mut drinking);
    let drink_volume = minimum_drink_volume(&registries);
    let water = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut drinking,
        drink_volume,
        FLUID_WATER,
        drink_volume,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("drinking-duration water fixture failed: {error}"));
    let _drinking_outcome = validate_drink(&registries, &drinking, water, drink_volume)
        .unwrap_or_else(|error| panic!("drinking-duration validation failed: {error}"))
        .commit(&mut drinking)
        .unwrap_or_else(|error| panic!("drinking-duration commit failed: {error}"));
    let mut tampered = serde_json::to_value(SaveEnvelope::new(&registries, &drinking))
        .unwrap_or_else(|error| panic!("drinking-duration serialization failed: {error}"));
    let completes_at =
        tampered["state"]["systems"]["player_work"]["active"]["Drinking"]["work"]["completes_at"]
            .as_u64()
            .unwrap_or_else(|| {
                panic!("drinking-duration completion tick was not serialized as u64")
            });
    tampered["state"]["systems"]["player_work"]["active"]["Drinking"]["work"]["completes_at"] =
        serde_json::json!(completes_at + 1);
    let tampered: LoadedSaveEnvelope = serde_json::from_value(tampered)
        .unwrap_or_else(|error| panic!("drinking-duration tamper decode failed: {error}"));
    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::PlayerWork(
            PlayerWorkValidationError::DrinkingDurationMismatch
        )))
    );

    let mut tampered = serde_json::to_value(SaveEnvelope::new(&registries, &drinking))
        .unwrap_or_else(|error| panic!("drinking-volume serialization failed: {error}"));
    tampered["state"]["systems"]["player_work"]["active"]["Drinking"]["work"]["volume"] =
        serde_json::json!(0_u64);
    let tampered: LoadedSaveEnvelope = serde_json::from_value(tampered)
        .unwrap_or_else(|error| panic!("drinking-volume tamper decode failed: {error}"));
    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::PlayerWork(
            PlayerWorkValidationError::DrinkingVolumeInvalid {
                volume: Volume::ZERO,
            }
        )))
    );
}

#[test]
fn multi_tick_drinking_round_trip_preserves_fractional_absorption_exactly() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("multi-tick drinking survival setup failed: {error}"));
    let physiology = registries.survival().physiology();
    let hydration_before = physiology
        .maximum_hydration()
        .checked_sub(Volume::from_microliters(500_000))
        .unwrap_or_else(|| panic!("multi-tick drinking hydration fixture underflowed"));
    let expected_revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        expected_revision,
        expected_revision + 1,
        player_record(
            physiology.maximum_metabolic_energy(),
            hydration_before,
            Vitality::MAXIMUM,
            NutritionReserves::from_parts_per_million(
                NUTRITION_PARTS_PER_MILLION,
                NUTRITION_PARTS_PER_MILLION,
                NUTRITION_PARTS_PER_MILLION,
            ),
            0,
        ),
    );
    let volume = minimum_drink_volume(&registries);
    let duration = physiology
        .direct_consumption()
        .drink_duration(volume)
        .unwrap_or_else(|| panic!("minimum drink has no authored duration"));
    assert!(duration.value() > 1);
    let store = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        volume,
        FLUID_WATER,
        volume,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("multi-tick drinking water fixture failed: {error}"));
    let outcome = validate_drink(&registries, &state, store, volume)
        .unwrap_or_else(|error| panic!("multi-tick drinking validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("multi-tick drinking commit failed: {error}"));
    assert_eq!(outcome.hydration_offered(), volume);

    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("multi-tick drinking first tick failed: {error}"));
    let first_tick_hydration = hydration_before
        .checked_add(Volume::from_microliters(
            volume.microliters() / duration.value(),
        ))
        .and_then(|value| value.checked_sub(physiology.hydration_loss_per_tick()))
        .unwrap_or_else(|| panic!("multi-tick first hydration expectation failed"));
    assert_eq!(
        assess_survival(&registries, &state)
            .unwrap_or_else(|| panic!("multi-tick player disappeared after first tick"))
            .hydration(),
        first_tick_hydration
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("multi-tick in-progress state audit failed: {error}"));

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("multi-tick drinking serialization failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("multi-tick drinking decode failed: {error}"));
    let mut loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("multi-tick drinking trusted load failed: {error}"));
    let mut uninterrupted = state.clone();
    assert_eq!(loaded, uninterrupted);

    for _ in 1..duration.value() {
        let _ = advance_tick(&registries, &mut loaded).unwrap_or_else(|error| {
            panic!("loaded multi-tick drinking continuation failed: {error}")
        });
        let _ = advance_tick(&registries, &mut uninterrupted).unwrap_or_else(|error| {
            panic!("uninterrupted multi-tick drinking continuation failed: {error}")
        });
    }
    assert_eq!(loaded, uninterrupted);
    assert_eq!(loaded.player_work().active(), None);
    assert_eq!(
        assess_survival(&registries, &loaded)
            .unwrap_or_else(|| panic!("multi-tick player disappeared after completion"))
            .hydration(),
        hydration_before
            .checked_add(volume)
            .and_then(|value| {
                value.checked_sub(Volume::from_microliters(
                    physiology
                        .hydration_loss_per_tick()
                        .microliters()
                        .checked_mul(duration.value())
                        .unwrap_or_else(|| panic!("multi-tick hydration loss overflowed")),
                ))
            })
            .unwrap_or_else(|| panic!("multi-tick final hydration expectation failed"))
    );
}

#[test]
fn dead_player_pending_consumption_cancels_on_next_tick_with_player_work() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("dead-consumption survival setup failed: {error}"));
    let volume = minimum_drink_volume(&registries);
    let store = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        volume,
        FLUID_WATER,
        volume,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("dead-consumption water fixture failed: {error}"));
    let _ = validate_drink(&registries, &state, store, volume)
        .unwrap_or_else(|error| panic!("dead-consumption drink validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("dead-consumption drink commit failed: {error}"));
    let player = state
        .survival()
        .player()
        .copied()
        .unwrap_or_else(|| panic!("dead-consumption player disappeared"));
    let expected_revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        expected_revision,
        expected_revision + 1,
        player_record(
            player.metabolic_energy(),
            player.hydration(),
            Vitality::ZERO,
            player.nutrition(),
            player.vitality_recovery_remainder(),
        ),
    );
    let frozen_revision = state.survival().revision();
    let frozen_player = state.survival().player().copied();

    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("dead-consumption cancellation tick failed: {error}"));
    assert_eq!(state.survival().revision(), frozen_revision + 1);
    assert_eq!(state.survival().player().copied(), frozen_player);
    assert_eq!(state.survival().pending_direct_consumption(), None);
    assert_eq!(state.player_work().active(), None);
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("dead-consumption final audit failed: {error}"));
}

#[test]
fn obsolete_save_without_direct_consumption_state_is_rejected_during_decode() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("current-schema survival setup failed: {error}"));
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("current-schema survival serialization failed: {error}"));
    let removed = encoded["state"]["systems"]["survival"]
        .as_object_mut()
        .unwrap_or_else(|| panic!("serialized survival state is not an object"))
        .remove("direct_consumption");
    assert!(
        removed.is_some(),
        "current schema must serialize direct-consumption state"
    );
    assert!(
        serde_json::from_value::<LoadedSaveEnvelope>(encoded).is_err(),
        "save payloads predating required direct-consumption state must not receive compatibility defaults"
    );
}

#[test]
fn drinking_near_capacity_absorbs_after_same_tick_basal_loss() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state).unwrap_or_else(|error| {
        panic!("partial-hydration survival initialization failed: {error}")
    });
    let physiology = registries.survival().physiology();
    let hydration_before = physiology
        .maximum_hydration()
        .checked_sub(Volume::from_microliters(1))
        .unwrap_or_else(|| panic!("partial-hydration fixture underflowed"));
    let expected_revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        expected_revision,
        expected_revision + 1,
        player_record(
            physiology.maximum_metabolic_energy(),
            hydration_before,
            Vitality::MAXIMUM,
            NutritionReserves::from_parts_per_million(
                NUTRITION_PARTS_PER_MILLION,
                NUTRITION_PARTS_PER_MILLION,
                NUTRITION_PARTS_PER_MILLION,
            ),
            0,
        ),
    );
    let drink_volume = minimum_drink_volume(&registries);
    let store = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        drink_volume,
        FLUID_WATER,
        drink_volume,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("partial-hydration water fixture failed: {error}"));
    let outcome = validate_drink(&registries, &state, store, drink_volume)
        .unwrap_or_else(|error| panic!("partial-hydration drink validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("partial-hydration drink commit failed: {error}"));

    assert_eq!(outcome.volume(), drink_volume);
    assert_eq!(outcome.hydration_offered(), drink_volume);
    assert_eq!(
        assess_survival(&registries, &state)
            .unwrap_or_else(|| panic!("partial-hydration survival state disappeared"))
            .hydration(),
        hydration_before
    );
    assert_eq!(
        state
            .fluid()
            .get_store(store)
            .map(|record| record.stored_volume()),
        Some(Volume::ZERO)
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("partial-hydration post-drink audit failed: {error}"));
    let duration = physiology
        .direct_consumption()
        .drink_duration(drink_volume)
        .unwrap_or_else(|| panic!("minimum drink duration disappeared"));
    assert_eq!(
        finish_direct_consumption(&registries, &mut state),
        duration.value()
    );
    assert_eq!(
        assess_survival(&registries, &state)
            .unwrap_or_else(|| panic!("partial-hydration survival state disappeared after drink"))
            .hydration(),
        physiology.maximum_hydration()
    );
}

#[test]
fn drink_hydration_first_covers_same_tick_hydration_shortfall() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("drink-shortfall survival setup failed: {error}"));
    let physiology = registries.survival().physiology();
    let hydration_before = Volume::from_microliters(120);
    let expected_revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        expected_revision,
        expected_revision + 1,
        player_record(
            physiology.maximum_metabolic_energy(),
            hydration_before,
            Vitality::MAXIMUM,
            NutritionReserves::from_parts_per_million(
                NUTRITION_PARTS_PER_MILLION,
                NUTRITION_PARTS_PER_MILLION,
                NUTRITION_PARTS_PER_MILLION,
            ),
            0,
        ),
    );
    let volume = minimum_drink_volume(&registries);
    let store = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        volume,
        FLUID_WATER,
        volume,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("drink-shortfall water fixture failed: {error}"));
    let outcome = validate_drink(&registries, &state, store, volume)
        .unwrap_or_else(|error| panic!("drink-shortfall validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("drink-shortfall commit failed: {error}"));
    let duration = physiology
        .direct_consumption()
        .drink_duration(volume)
        .unwrap_or_else(|| panic!("minimum drink duration disappeared"));
    assert_eq!(
        finish_direct_consumption(&registries, &mut state),
        duration.value()
    );
    let total_loss = Volume::from_microliters(
        physiology
            .hydration_loss_per_tick()
            .microliters()
            .checked_mul(duration.value())
            .unwrap_or_else(|| panic!("drink-shortfall hydration loss overflowed")),
    );
    let expected_after = hydration_before
        .checked_add(outcome.hydration_offered())
        .and_then(|value| value.checked_sub(total_loss))
        .unwrap_or(Volume::ZERO);
    assert_eq!(
        assess_survival(&registries, &state)
            .unwrap_or_else(|| panic!("drink-shortfall player disappeared"))
            .hydration(),
        expected_after
    );
}

#[test]
fn eating_at_full_reserves_absorbs_as_basal_cost_creates_capacity() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("full-reserve survival initialization failed: {error}"));
    let physiology = registries.survival().physiology();
    let meal_mass = minimum_meal_mass(&registries);
    let stockpile = add_solid_stockpile_for_test(&mut state, meal_mass)
        .unwrap_or_else(|error| panic!("full-reserve food stockpile failed: {error}"));
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD),
        meal_mass,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("full-reserve food lot failed: {error}"));
    let outcome = validate_eat(
        &registries,
        &state,
        stockpile,
        &[MaterialLotSelection::new(lot, meal_mass)],
    )
    .unwrap_or_else(|error| panic!("full-reserve eating should remain useful over time: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("full-reserve eating commit failed: {error}"));
    assert_eq!(state.inventory().get_lot(lot), None);
    assert_eq!(finish_direct_consumption(&registries, &mut state), 1);
    let net_cost = physiology
        .basal_energy_cost_per_tick()
        .checked_sub(outcome.energy_offered())
        .unwrap_or(Energy::ZERO);
    assert_eq!(
        assess_survival(&registries, &state)
            .unwrap_or_else(|| panic!("full-reserve player disappeared after eating"))
            .metabolic_energy(),
        physiology
            .maximum_metabolic_energy()
            .checked_sub(net_cost)
            .unwrap_or_else(|| panic!("full-reserve eating expectation underflowed"))
    );
}

#[test]
fn nutrition_credit_uses_consumed_food_even_when_metabolic_reserve_is_full() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("nutrition-clamp survival initialization failed: {error}"));
    let physiology = registries.survival().physiology();
    let expected_revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        expected_revision,
        expected_revision + 1,
        player_record(
            physiology.maximum_metabolic_energy(),
            physiology.maximum_hydration(),
            Vitality::MAXIMUM,
            NutritionReserves::from_parts_per_million(0, 0, 0),
            0,
        ),
    );
    let meal_mass = minimum_meal_mass(&registries);
    let stockpile = add_solid_stockpile_for_test(&mut state, meal_mass)
        .unwrap_or_else(|error| panic!("nutrition-clamp stockpile failed: {error}"));
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD),
        meal_mass,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("nutrition-clamp food lot failed: {error}"));

    let outcome = validate_eat(
        &registries,
        &state,
        stockpile,
        &[MaterialLotSelection::new(lot, meal_mass)],
    )
    .unwrap_or_else(|error| panic!("nutrition-clamp eating validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("nutrition-clamp eating commit failed: {error}"));

    assert_eq!(
        outcome.energy_offered(),
        registries
            .survival()
            .get_food(CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD))
            .unwrap_or_else(|| panic!("grain food definition disappeared"))
            .dietary_energy_for_mass(meal_mass)
    );
    assert!(outcome.nutrition_offered().get(FoodCategory::Grain) > 0);
    assert_eq!(
        assess_survival(&registries, &state)
            .unwrap_or_else(|| panic!("nutrition-clamp survival state disappeared at admission"))
            .nutrition()
            .get(FoodCategory::Grain),
        0
    );
    assert_eq!(finish_direct_consumption(&registries, &mut state), 1);
    let expected_nutrition = outcome
        .nutrition_offered()
        .get(FoodCategory::Grain)
        .min(NUTRITION_PARTS_PER_MILLION)
        .saturating_sub(physiology.nutrition().decay_ppm_per_tick());
    assert_eq!(
        assess_survival(&registries, &state)
            .unwrap_or_else(|| panic!("nutrition-clamp survival state disappeared"))
            .nutrition()
            .get(FoodCategory::Grain),
        expected_nutrition
    );
}

#[test]
fn meal_below_authored_intake_minimum_is_rejected_without_consumption() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_and_spend_reserves(&registries, &mut state);
    let minimum = minimum_meal_mass(&registries);
    let requested = minimum
        .checked_sub(Mass::from_milligrams(1))
        .unwrap_or_else(|| panic!("meal minimum fixture underflowed"));
    let stockpile = add_solid_stockpile_for_test(&mut state, minimum)
        .unwrap_or_else(|error| panic!("meal-minimum stockpile failed: {error}"));
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD),
        minimum,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("meal-minimum food lot failed: {error}"));
    let before = state.clone();

    assert_eq!(
        validate_eat(
            &registries,
            &state,
            stockpile,
            &[MaterialLotSelection::new(lot, requested)],
        )
        .err(),
        Some(EatError::MealMassBelowIntakeMinimum {
            mass: requested,
            minimum,
        })
    );
    assert_eq!(state, before);
}

#[test]
fn very_large_meal_is_rejected_by_authored_intake_limit_without_consumption() {
    const MEAL_MASS_MG: u64 = 7_000_000_000;

    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("large-meal survival initialization failed: {error}"));
    let physiology = registries.survival().physiology();
    let expected_revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        expected_revision,
        expected_revision + 1,
        player_record(
            physiology.maximum_metabolic_energy(),
            physiology.maximum_hydration(),
            Vitality::MAXIMUM,
            NutritionReserves::from_parts_per_million(0, 0, 0),
            0,
        ),
    );
    let stockpile = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(MEAL_MASS_MG))
        .unwrap_or_else(|error| panic!("large-meal stockpile failed: {error}"));
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD),
        Mass::from_milligrams(MEAL_MASS_MG),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("large-meal food lot failed: {error}"));
    let before = state.clone();

    assert_eq!(
        validate_eat(
            &registries,
            &state,
            stockpile,
            &[MaterialLotSelection::new(
                lot,
                Mass::from_milligrams(MEAL_MASS_MG),
            )],
        )
        .err(),
        Some(EatError::MealMassExceedsIntakeLimit {
            mass: Mass::from_milligrams(MEAL_MASS_MG),
            maximum: physiology.direct_consumption().maximum_meal_mass(),
        })
    );
    assert_eq!(state, before);
}

#[test]
fn drinking_at_full_hydration_absorbs_as_basal_loss_creates_capacity() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("full-hydration survival initialization failed: {error}"));
    let physiology = registries.survival().physiology();
    let volume = minimum_drink_volume(&registries);
    let store = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        volume,
        FLUID_WATER,
        volume,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("full-hydration water fixture failed: {error}"));
    let outcome = validate_drink(&registries, &state, store, volume)
        .unwrap_or_else(|error| {
            panic!("full-hydration drinking should remain useful over time: {error}")
        })
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("full-hydration drinking commit failed: {error}"));
    assert_eq!(outcome.hydration_offered(), volume);
    let duration = physiology
        .direct_consumption()
        .drink_duration(volume)
        .unwrap_or_else(|| panic!("minimum drink duration disappeared"));
    assert_eq!(
        finish_direct_consumption(&registries, &mut state),
        duration.value()
    );
    assert_eq!(
        assess_survival(&registries, &state)
            .unwrap_or_else(|| panic!("full-hydration player disappeared after drinking"))
            .hydration(),
        physiology.maximum_hydration()
    );
}
