//! Fatal interruption, temperature, attention, and intake-limit contracts.

use super::*;

#[test]
fn death_during_drinking_releases_attention_and_discards_unabsorbed_intake_immediately() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("death-during-drink survival setup failed: {error}"));
    let physiology = registries.survival().physiology();
    let expected_revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        expected_revision,
        expected_revision + 1,
        player_record(
            Energy::ZERO,
            physiology.maximum_hydration(),
            Vitality::from_parts_per_million_unchecked(
                physiology.starvation_vitality_loss_ppm_per_tick(),
            ),
            NutritionReserves::FULL,
            0,
        ),
    );
    let volume = physiology.direct_consumption().maximum_drink_volume();
    let store = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        volume,
        FLUID_WATER,
        volume,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("death-during-drink water fixture failed: {error}"));
    let _ = validate_drink(&registries, &state, store, volume)
        .unwrap_or_else(|error| panic!("death-during-drink validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("death-during-drink commit failed: {error}"));
    let PlayerWork::Drinking { work } = state
        .player_work()
        .active()
        .unwrap_or_else(|| panic!("death-during-drink did not claim player attention"))
    else {
        panic!("death-during-drink claimed the wrong player-work kind");
    };
    assert!(work.completes_at().value() > state.tick().value() + 1);

    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("death-during-drink fatal tick failed: {error}"));
    assert_eq!(
        state.survival().player().map(|player| player.vitality()),
        Some(Vitality::ZERO)
    );
    assert_eq!(state.player_work().active(), None);
    assert!(state.survival().pending_direct_consumption().is_none());
    let hydration_at_death = state
        .survival()
        .player()
        .unwrap_or_else(|| panic!("death-during-drink player disappeared"))
        .hydration();
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("death-during-drink fatal state failed audit: {error}"));

    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("death-during-drink post-death tick failed: {error}"));
    assert_eq!(state.player_work().active(), None);
    assert!(state.survival().pending_direct_consumption().is_none());
    assert_eq!(
        state
            .survival()
            .player()
            .map(|player| (player.vitality(), player.hydration())),
        Some((Vitality::ZERO, hydration_at_death))
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("death-during-drink cleanup state failed audit: {error}"));
}

#[test]
fn load_rejects_dead_player_with_pending_direct_consumption() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("dead-pending-load survival setup failed: {error}"));
    let volume = registries
        .survival()
        .physiology()
        .direct_consumption()
        .maximum_drink_volume();
    let store = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        volume,
        FLUID_WATER,
        volume,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("dead-pending-load water fixture failed: {error}"));
    let _ = validate_drink(&registries, &state, store, volume)
        .unwrap_or_else(|error| panic!("dead-pending-load drink validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("dead-pending-load drink commit failed: {error}"));
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("dead-pending-load serialization failed: {error}"));
    encoded["state"]["systems"]["survival"]["player"]["vitality"] = serde_json::json!(0);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("dead-pending-load decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::Survival(
            SurvivalValidationError::PendingConsumptionForDeadPlayer
        )))
    );
}

#[test]
fn direct_consumption_rejects_unsafe_food_and_water_temperatures_without_mutation() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_and_spend_reserves(&registries, &mut state);
    let hot_temperature = Temperature::from_millikelvin(333_151);
    let food_source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(10))
        .unwrap_or_else(|error| panic!("hot food stockpile failed: {error}"));
    let food = deposit_lot_for_test(
        &registries,
        &mut state,
        food_source,
        CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD),
        Mass::from_milligrams(10),
        hot_temperature,
    )
    .unwrap_or_else(|error| panic!("hot food fixture failed: {error}"));
    let drink_volume = minimum_drink_volume(&registries);
    let water = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        drink_volume,
        FLUID_WATER,
        drink_volume,
        hot_temperature,
    )
    .unwrap_or_else(|error| panic!("hot water fixture failed: {error}"));
    let before = state.clone();

    assert_eq!(
        validate_eat(
            &registries,
            &state,
            food_source,
            &[MaterialLotSelection::new(food, Mass::from_milligrams(1))],
        )
        .err(),
        Some(EatError::TemperatureOutsideConsumptionRange {
            lot: food,
            temperature: hot_temperature,
            minimum: Temperature::from_millikelvin(273_150),
            maximum: Temperature::from_millikelvin(333_150),
        })
    );
    assert_eq!(
        validate_drink(&registries, &state, water, drink_volume).err(),
        Some(DrinkError::TemperatureOutsideConsumptionRange {
            store: water,
            temperature: hot_temperature,
            minimum: Temperature::from_millikelvin(273_150),
            maximum: Temperature::from_millikelvin(333_150),
        })
    );
    assert_eq!(state, before);
}

#[test]
fn direct_consumption_claims_quantity_scaled_player_attention() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_and_spend_reserves(&registries, &mut state);
    let stockpile = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(300_000))
        .unwrap_or_else(|error| panic!("attention meal stockpile failed: {error}"));
    let food = deposit_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD),
        Mass::from_milligrams(200_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("attention meal fixture failed: {error}"));
    let drink_volume = minimum_drink_volume(&registries);
    let water = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        drink_volume,
        FLUID_WATER,
        drink_volume,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("attention drink fixture failed: {error}"));

    let meal = validate_eat(
        &registries,
        &state,
        stockpile,
        &[MaterialLotSelection::new(
            food,
            Mass::from_milligrams(100_000),
        )],
    )
    .unwrap_or_else(|error| panic!("attention meal validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("attention meal commit failed: {error}"));

    let active = state
        .player_work()
        .active()
        .unwrap_or_else(|| panic!("eating did not claim player attention"));
    let PlayerWork::Eating { work } = active else {
        panic!("eating claimed wrong player-work kind: {active:?}");
    };
    assert_eq!(meal.completes_at(), work.completes_at());
    assert_eq!(work.mass(), Mass::from_milligrams(100_000));
    assert_eq!(
        work.completes_at().value() - work.started_at().value(),
        registries
            .survival()
            .physiology()
            .direct_consumption()
            .meal_duration(work.mass())
            .unwrap_or_else(|| panic!("authored meal duration disappeared"))
            .value()
    );
    let before_rejected_actions = state.clone();
    assert_eq!(
        validate_eat(
            &registries,
            &state,
            stockpile,
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
    assert_eq!(state, before_rejected_actions);

    let duration = work.completes_at().value() - state.tick().value();
    for _ in 0..duration {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("attention meal tick failed: {error}"));
    }
    assert_eq!(state.player_work().active(), None);
    assert!(
        validate_drink(&registries, &state, water, drink_volume).is_ok(),
        "direct drinking must become available after the authored meal interval finishes"
    );
}

#[test]
fn drinking_rejects_volume_above_authored_intake_limit_without_consumption() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_and_spend_reserves(&registries, &mut state);
    let maximum = registries
        .survival()
        .physiology()
        .direct_consumption()
        .maximum_drink_volume();
    let requested = maximum
        .checked_add(Volume::from_microliters(1))
        .unwrap_or_else(|| panic!("drink-limit fixture overflowed"));
    let store = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        requested,
        FLUID_WATER,
        requested,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("drink-limit water fixture failed: {error}"));
    let before = state.clone();

    assert_eq!(
        validate_drink(&registries, &state, store, requested).err(),
        Some(DrinkError::DrinkVolumeExceedsIntakeLimit {
            volume: requested,
            maximum,
        })
    );
    assert_eq!(state, before);
}

#[test]
fn drinking_rejects_volume_below_authored_intake_minimum_without_consumption() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_and_spend_reserves(&registries, &mut state);
    let minimum = registries
        .survival()
        .physiology()
        .direct_consumption()
        .minimum_drink_volume();
    let requested = minimum
        .checked_sub(Volume::from_microliters(1))
        .unwrap_or_else(|| panic!("drink-minimum fixture underflowed"));
    let store = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        minimum,
        FLUID_WATER,
        minimum,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("drink-minimum water fixture failed: {error}"));
    let before = state.clone();

    assert_eq!(
        validate_drink(&registries, &state, store, requested).err(),
        Some(DrinkError::DrinkVolumeBelowIntakeMinimum {
            volume: requested,
            minimum,
        })
    );
    assert_eq!(state, before);
}
