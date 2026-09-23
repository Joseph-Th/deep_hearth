//! Trusted-load rejection for independently persisted direct-consumption continuation state.

use super::*;

fn decode_tampered(encoded: serde_json::Value, context: &str) -> LoadedSaveEnvelope {
    serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("{context} tampered save failed to decode: {error}"))
}

fn eating_state(registries: &Registries, mass: Mass) -> AppState {
    let mut state = AppState::new();
    initialize_and_spend_reserves(registries, &mut state);
    let stockpile = add_solid_stockpile_for_test(&mut state, mass)
        .unwrap_or_else(|error| panic!("pending-meal stockpile failed: {error}"));
    let food = deposit_lot_for_test(
        registries,
        &mut state,
        stockpile,
        CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD),
        mass,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("pending-meal food failed: {error}"));
    let _ = validate_eat(
        registries,
        &state,
        stockpile,
        &[MaterialLotSelection::new(food, mass)],
    )
    .unwrap_or_else(|error| panic!("pending-meal validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("pending-meal commit failed: {error}"));
    state
}

#[test]
fn trusted_load_rejects_pending_meal_with_forged_spoiled_storage_history() {
    let registries = build_registries();
    let state = eating_state(&registries, Mass::from_milligrams(2));

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("pending freshness serialization failed: {error}"));
    let shelf_life = registries
        .survival()
        .get_food(CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD))
        .unwrap_or_else(|| panic!("grain food definition disappeared"))
        .shelf_life();
    encoded["state"]["systems"]["survival"]["direct_consumption"]["pending"]["Eating"]["consumed"]
        [0]["storage_history"]["ambient_age_parts"] = serde_json::json!(
        u128::from(shelf_life.value()) * crate::inventory::STORAGE_AGE_PARTS_PER_TICK
    );

    assert_eq!(
        decode_tampered(encoded, "pending-spoiled-freshness").into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Survival(
            SurvivalValidationError::PendingEatingFreshnessInvalid
        )))
    );
}

#[test]
fn pending_meal_replay_keeps_admission_freshness_after_source_storage_improves() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_and_spend_reserves(&registries, &mut state);
    let meal_mass = Mass::from_milligrams(2);
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20_000_000))
        .unwrap_or_else(|error| panic!("pending-meal source stockpile failed: {error}"));
    let food = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD),
        meal_mass,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("pending-meal food failed: {error}"));
    let enclosure_source =
        add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2_400_000))
            .unwrap_or_else(|error| panic!("pending-meal enclosure source failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        enclosure_source,
        CommodityKey::new(MATERIAL_WOOD, FORM_CHEST_BODY),
        Mass::from_milligrams(2_400_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("pending-meal enclosure body failed: {error}"));

    let _ = validate_eat(
        &registries,
        &state,
        source,
        &[MaterialLotSelection::new(food, meal_mass)],
    )
    .unwrap_or_else(|error| panic!("pending-meal validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("pending-meal commit failed: {error}"));
    validate_build_storage_enclosure(
        &registries,
        &state,
        STORAGE_TIMBER_PROVISIONS_CHEST,
        source,
        enclosure_source,
    )
    .unwrap_or_else(|error| panic!("pending-meal enclosure validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("pending-meal enclosure commit failed: {error}"));

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("pending-meal enclosure serialization failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("pending-meal enclosure decode failed: {error}"));
    let loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("pending-meal enclosure trusted load failed: {error}"));
    assert_eq!(loaded, state);

    let mut tampered = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("pending-meal history serialization failed: {error}"));
    let pending =
        &mut tampered["state"]["systems"]["survival"]["direct_consumption"]["pending"]["Eating"];
    let started_at = pending["started_at"]
        .as_u64()
        .unwrap_or_else(|| panic!("pending meal start tick is not a u64"));
    assert!(started_at > 0);
    pending["consumed"][0]["storage_history"]["last_transition_at"] =
        serde_json::json!(started_at - 1);

    assert_eq!(
        decode_tampered(tampered, "pending-pre-admission-history").into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Survival(
            SurvivalValidationError::PendingEatingFreshnessInvalid
        )))
    );
}

#[test]
fn aged_fresh_pending_meal_round_trips_with_admission_history_intact() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_and_spend_reserves(&registries, &mut state);
    let mass = Mass::from_milligrams(2);
    let stockpile = add_solid_stockpile_for_test(&mut state, mass)
        .unwrap_or_else(|error| panic!("aged pending-meal stockpile failed: {error}"));
    let food = deposit_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD),
        mass,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("aged pending-meal food failed: {error}"));
    let shelf_life = registries
        .survival()
        .get_food(CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD))
        .unwrap_or_else(|| panic!("grain food definition disappeared"))
        .shelf_life();
    let age = TickSpan::new((shelf_life.value() / 2).max(1));
    let aged_at = state
        .tick()
        .checked_add_span(age)
        .unwrap_or_else(|| panic!("aged pending-meal clock overflowed"));
    apply_clock_advance(&mut state, aged_at);
    assert!(matches!(
        assess_food_freshness(&registries, &state, food),
        Ok(FoodFreshness::Fresh { age: actual, .. }) if actual == age
    ));
    let _ = validate_eat(
        &registries,
        &state,
        stockpile,
        &[MaterialLotSelection::new(food, mass)],
    )
    .unwrap_or_else(|error| panic!("aged pending-meal validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("aged pending-meal commit failed: {error}"));

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("aged pending-meal serialization failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("aged pending-meal decode failed: {error}"));
    let loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("aged pending-meal trusted load failed: {error}"));
    assert_eq!(loaded, state);
}

fn drinking_state(registries: &Registries, volume: Volume) -> AppState {
    let mut state = AppState::new();
    initialize_and_spend_reserves(registries, &mut state);
    let store = add_fluid_store_with_contents_for_fixture(
        registries,
        &mut state,
        volume,
        FLUID_WATER,
        volume,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("pending-drink fixture failed: {error}"));
    let _ = validate_drink(registries, &state, store, volume)
        .unwrap_or_else(|error| panic!("pending-drink validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("pending-drink commit failed: {error}"));
    state
}

#[test]
fn trusted_load_rejects_future_and_elapsed_pending_consumption_schedules() {
    let registries = build_registries();
    let volume = minimum_drink_volume(&registries);
    let state = drinking_state(&registries, volume);
    let current = state.tick().value();

    let mut future = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("future-pending serialization failed: {error}"));
    future["state"]["systems"]["survival"]["direct_consumption"]["pending"]["Drinking"]["started_at"] =
        serde_json::json!(current + 1);
    future["state"]["systems"]["survival"]["direct_consumption"]["pending"]["Drinking"]["completes_at"] =
        serde_json::json!(current + 2);
    assert_eq!(
        decode_tampered(future, "future-pending").into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Survival(
            SurvivalValidationError::PendingConsumptionScheduleInvalid
        )))
    );

    let maximum = registries
        .survival()
        .physiology()
        .direct_consumption()
        .maximum_drink_volume();
    let mut elapsed_state = drinking_state(&registries, maximum);
    let PlayerWork::Drinking { work } = elapsed_state
        .player_work()
        .active()
        .unwrap_or_else(|| panic!("elapsed-pending fixture has no drinking work"))
    else {
        panic!("elapsed-pending fixture has wrong player work")
    };
    assert!(
        work.completes_at().value() > elapsed_state.tick().value() + 1,
        "maximum drink must span enough ticks to construct an elapsed pending schedule"
    );
    let _ = advance_tick(&registries, &mut elapsed_state)
        .unwrap_or_else(|error| panic!("elapsed-pending setup tick failed: {error}"));
    let current = elapsed_state.tick().value();
    let mut elapsed = serde_json::to_value(SaveEnvelope::new(&registries, &elapsed_state))
        .unwrap_or_else(|error| panic!("elapsed-pending serialization failed: {error}"));
    elapsed["state"]["systems"]["survival"]["direct_consumption"]["pending"]["Drinking"]["completes_at"] =
        serde_json::json!(current);
    assert_eq!(
        decode_tampered(elapsed, "elapsed-pending").into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Survival(
            SurvivalValidationError::PendingConsumptionScheduleInvalid
        )))
    );
}

#[test]
fn trusted_load_rejects_pending_drink_duration_mismatch() {
    let registries = build_registries();
    let minimum = minimum_drink_volume(&registries);
    let state = drinking_state(&registries, minimum);
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("pending-drink-duration serialization failed: {error}"));
    let completes_at = encoded["state"]["systems"]["survival"]["direct_consumption"]["pending"]
        ["Drinking"]["completes_at"]
        .as_u64()
        .unwrap_or_else(|| panic!("pending drink completion tick is not a u64"));
    encoded["state"]["systems"]["survival"]["direct_consumption"]["pending"]["Drinking"]["completes_at"] =
        serde_json::json!(completes_at + 1);

    assert_eq!(
        decode_tampered(encoded, "pending-drink-duration").into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Survival(
            SurvivalValidationError::PendingDrinkingVolumeInvalid
        )))
    );
}

#[test]
fn trusted_load_rejects_pending_drink_below_authored_minimum() {
    let registries = build_registries();
    let minimum = minimum_drink_volume(&registries);
    let state = drinking_state(&registries, minimum);
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("pending-drink-minimum serialization failed: {error}"));
    let below_minimum = minimum
        .checked_sub(Volume::from_microliters(1))
        .unwrap_or_else(|| panic!("pending-drink minimum fixture underflowed"));
    encoded["state"]["systems"]["survival"]["direct_consumption"]["pending"]["Drinking"]["volume"] =
        serde_json::json!(below_minimum.microliters());

    assert_eq!(
        decode_tampered(encoded, "pending-drink-minimum").into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Survival(
            SurvivalValidationError::PendingDrinkingVolumeInvalid
        )))
    );
}

#[test]
fn trusted_load_rejects_pending_consumption_larger_than_terminal_accounting() {
    let registries = build_registries();

    let mut eating = AppState::new();
    initialize_and_spend_reserves(&registries, &mut eating);
    let stockpile = add_solid_stockpile_for_test(&mut eating, Mass::from_milligrams(2))
        .unwrap_or_else(|error| panic!("pending-meal accounting stockpile failed: {error}"));
    let food = deposit_lot_for_test(
        &registries,
        &mut eating,
        stockpile,
        CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD),
        Mass::from_milligrams(2),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("pending-meal accounting food failed: {error}"));
    let _ = validate_eat(
        &registries,
        &eating,
        stockpile,
        &[MaterialLotSelection::new(food, Mass::from_milligrams(2))],
    )
    .unwrap_or_else(|error| panic!("pending-meal accounting validation failed: {error}"))
    .commit(&mut eating)
    .unwrap_or_else(|error| panic!("pending-meal accounting commit failed: {error}"));
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &eating))
        .unwrap_or_else(|error| panic!("pending-meal accounting serialization failed: {error}"));
    let consumed_matter = encoded["state"]["systems"]["survival"]["consumed_matter"]
        .as_object_mut()
        .unwrap_or_else(|| panic!("serialized consumed matter is not an object"));
    let consumed = consumed_matter
        .values_mut()
        .next()
        .unwrap_or_else(|| panic!("serialized pending meal has no terminal consumed matter"));
    *consumed = serde_json::json!(1_u64);
    assert_eq!(
        decode_tampered(encoded, "pending-meal-accounting").into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Survival(
            SurvivalValidationError::PendingEatingAccountingMismatch {
                material: MATERIAL_GRAIN,
            }
        )))
    );

    let minimum = minimum_drink_volume(&registries);
    let drinking = drinking_state(&registries, minimum);
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &drinking))
        .unwrap_or_else(|error| panic!("pending-drink accounting serialization failed: {error}"));
    let consumed_fluids = encoded["state"]["systems"]["survival"]["consumed_fluids"]
        .as_object_mut()
        .unwrap_or_else(|| panic!("serialized consumed fluids is not an object"));
    let consumed = consumed_fluids
        .values_mut()
        .next()
        .unwrap_or_else(|| panic!("serialized pending drink has no terminal consumed fluid"));
    *consumed = serde_json::json!(1_u64);
    assert_eq!(
        decode_tampered(encoded, "pending-drink-accounting").into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Survival(
            SurvivalValidationError::PendingDrinkingAccountingMismatch { fluid: FLUID_WATER }
        )))
    );
}

#[test]
fn trusted_load_rejects_noncanonical_pending_eating_baseline_order() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_and_spend_reserves(&registries, &mut state);
    let stockpile = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2))
        .unwrap_or_else(|error| panic!("pending-baseline-order stockpile failed: {error}"));
    let grain = deposit_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD),
        Mass::from_milligrams(1),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("pending-baseline-order grain failed: {error}"));
    let berries = deposit_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
        Mass::from_milligrams(1),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("pending-baseline-order berries failed: {error}"));
    let _ = validate_eat(
        &registries,
        &state,
        stockpile,
        &[
            MaterialLotSelection::new(grain, Mass::from_milligrams(1)),
            MaterialLotSelection::new(berries, Mass::from_milligrams(1)),
        ],
    )
    .unwrap_or_else(|error| panic!("pending-baseline-order validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("pending-baseline-order commit failed: {error}"));

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("pending-baseline-order serialization failed: {error}"));
    let baselines = encoded["state"]["systems"]["survival"]["direct_consumption"]["pending"]
        ["Eating"]["consumed_before"]
        .as_array_mut()
        .unwrap_or_else(|| panic!("pending eating baselines are not an array"));
    assert_eq!(baselines.len(), 2);
    baselines.reverse();
    let decoded = decode_tampered(encoded, "pending-baseline-order");

    assert!(matches!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Survival(
            SurvivalValidationError::PendingEatingAccountingMismatch { .. }
        )))
    ));
}

#[test]
fn maximum_pending_drink_round_trips_at_the_authored_boundary() {
    let registries = build_registries();
    let maximum = registries
        .survival()
        .physiology()
        .direct_consumption()
        .maximum_drink_volume();
    let state = drinking_state(&registries, maximum);
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("maximum pending drink serialization failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("maximum pending drink decode failed: {error}"));
    let loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("maximum pending drink trusted load failed: {error}"));

    assert_eq!(loaded, state);
}
