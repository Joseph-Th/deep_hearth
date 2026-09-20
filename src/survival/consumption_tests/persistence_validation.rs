//! Trusted-load rejection for independently persisted direct-consumption continuation state.

use super::*;

fn decode_tampered(encoded: serde_json::Value, context: &str) -> LoadedSaveEnvelope {
    serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("{context} tampered save failed to decode: {error}"))
}

fn drinking_state(registries: &Registries, seed: u64, volume: Volume) -> AppState {
    let mut state = AppState::new(WorldSeed::new(seed));
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
    let volume = Volume::from_microliters(1);
    let state = drinking_state(&registries, 0x5A70_0033, volume);
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
    let mut elapsed_state = drinking_state(&registries, 0x5A70_0034, maximum);
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
    let state = drinking_state(&registries, 0x5A70_0035, Volume::from_microliters(1));
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
fn trusted_load_rejects_pending_consumption_larger_than_terminal_accounting() {
    let registries = build_registries();

    let mut eating = AppState::new(WorldSeed::new(0x5A70_0036));
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

    let drinking = drinking_state(&registries, 0x5A70_0037, Volume::from_microliters(2));
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
    let mut state = AppState::new(WorldSeed::new(0x5A70_0039));
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
    let state = drinking_state(&registries, 0x5A70_0038, maximum);
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("maximum pending drink serialization failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("maximum pending drink decode failed: {error}"));
    let loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("maximum pending drink trusted load failed: {error}"));

    assert_eq!(loaded, state);
}
