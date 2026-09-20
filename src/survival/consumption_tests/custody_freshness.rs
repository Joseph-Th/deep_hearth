//! Terminal custody, varied meals, ordering, and storage-freshness contracts.

use super::*;

#[test]
fn eating_moves_exact_food_mass_into_consumption_boundary_and_round_trips() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5A70_0001));
    initialize_and_spend_reserves(&registries, &mut state);
    let stockpile = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000))
        .unwrap_or_else(|error| panic!("food stockpile fixture failed: {error}"));
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD),
        Mass::from_milligrams(200),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("food lot fixture failed: {error}"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("food pre-consumption matter accounting failed: {error}"));
    let survival_before = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("food fixture survival state is missing"));

    let token = validate_eat(
        &registries,
        &state,
        stockpile,
        &[MaterialLotSelection::new(lot, Mass::from_milligrams(100))],
    )
    .unwrap_or_else(|error| panic!("food validation failed: {error}"));
    let outcome = token
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("food commit failed: {error}"));

    let matter_after = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("food post-consumption matter accounting failed: {error}"));
    let survival_at_admission = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("food admission survival state is missing"));
    assert_eq!(matter_before.total(), matter_after.total());
    assert_eq!(matter_after.consumed(), AggregateMass::from_milligrams(100));
    assert_eq!(
        state.inventory().get_lot(lot).map(|record| record.mass()),
        Some(Mass::from_milligrams(100))
    );
    assert_eq!(outcome.total_mass(), Mass::from_milligrams(100));
    assert_eq!(outcome.portions().len(), 1);
    assert_eq!(outcome.portions()[0].lot(), lot);
    assert_eq!(outcome.portions()[0].mass(), Mass::from_milligrams(100));
    assert_eq!(outcome.portions()[0].category(), FoodCategory::Grain);
    assert!(outcome.nutrition_offered().total_ppm() > 0);
    assert_eq!(survival_at_admission, survival_before);
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("food in-progress audit failed: {error}"));
    assert_eq!(finish_direct_consumption(&registries, &mut state), 1);
    let survival_after = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("food completed survival state is missing"));
    assert!(survival_after.metabolic_energy() > survival_before.metabolic_energy());
    assert_eq!(
        survival_after.nutrition().get(FoodCategory::Grain),
        NUTRITION_PARTS_PER_MILLION
            - registries
                .survival()
                .physiology()
                .nutrition()
                .decay_ppm_per_tick()
    );
    assert_eq!(
        survival_after.nutrition().get(FoodCategory::Fruit),
        survival_before
            .nutrition()
            .get(FoodCategory::Fruit)
            .saturating_sub(
                registries
                    .survival()
                    .physiology()
                    .nutrition()
                    .decay_ppm_per_tick()
            )
    );
    assert_eq!(
        survival_after.nutrition().get(FoodCategory::Protein),
        survival_before
            .nutrition()
            .get(FoodCategory::Protein)
            .saturating_sub(
                registries
                    .survival()
                    .physiology()
                    .nutrition()
                    .decay_ppm_per_tick()
            )
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("food post-consumption audit failed: {error}"));

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("food save serialization failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("food save decode failed: {error}"));
    let loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("food save validation failed: {error}"));
    assert_eq!(loaded, state);
}

#[test]
fn varied_meal_consumes_multiple_foods_atomically_and_credits_each_category() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5A70_0004));
    initialize_and_spend_reserves(&registries, &mut state);
    let stockpile = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000))
        .unwrap_or_else(|error| panic!("varied meal stockpile fixture failed: {error}"));
    let grain = deposit_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD),
        Mass::from_milligrams(100),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("varied meal grain fixture failed: {error}"));
    let berries = deposit_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
        Mass::from_milligrams(100),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("varied meal berry fixture failed: {error}"));
    let meat = deposit_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        CommodityKey::new(MATERIAL_MEAT, FORM_FOOD),
        Mass::from_milligrams(100),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("varied meal meat fixture failed: {error}"));
    let before = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("varied meal survival state is missing"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("varied meal initial accounting failed: {error}"));
    let selections = [
        MaterialLotSelection::new(meat, Mass::from_milligrams(10)),
        MaterialLotSelection::new(grain, Mass::from_milligrams(10)),
        MaterialLotSelection::new(berries, Mass::from_milligrams(10)),
    ];

    let outcome = validate_eat(&registries, &state, stockpile, &selections)
        .unwrap_or_else(|error| panic!("varied meal validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("varied meal commit failed: {error}"));

    assert_eq!(outcome.total_mass(), Mass::from_milligrams(30));
    assert_eq!(outcome.portions().len(), 3);
    for category in [
        FoodCategory::Grain,
        FoodCategory::Fruit,
        FoodCategory::Protein,
    ] {
        assert!(outcome.nutrition_offered().get(category) > 0);
    }
    assert_eq!(
        assess_survival(&registries, &state)
            .unwrap_or_else(|| panic!("varied meal admission survival state disappeared")),
        before
    );
    assert_eq!(finish_direct_consumption(&registries, &mut state), 1);
    let after = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("varied meal survival state disappeared"));
    let decay = registries
        .survival()
        .physiology()
        .nutrition()
        .decay_ppm_per_tick();
    for category in [
        FoodCategory::Grain,
        FoodCategory::Fruit,
        FoodCategory::Protein,
    ] {
        let expected = before
            .nutrition()
            .get(category)
            .saturating_add(outcome.nutrition_offered().get(category))
            .min(NUTRITION_PARTS_PER_MILLION)
            .saturating_sub(decay);
        assert_eq!(after.nutrition().get(category), expected);
    }
    let matter_after = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("varied meal final accounting failed: {error}"));
    assert_eq!(matter_after.total(), matter_before.total());
    assert_eq!(
        matter_before
            .consumed()
            .checked_add(AggregateMass::from_milligrams(30)),
        Some(matter_after.consumed())
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("varied meal final audit failed: {error}"));
}

#[test]
fn meal_result_is_independent_of_selection_order() {
    let registries = build_registries();
    let mut base = AppState::new(WorldSeed::new(0x5A70_0006));
    initialize_and_spend_reserves(&registries, &mut base);
    let stockpile = add_solid_stockpile_for_test(&mut base, Mass::from_milligrams(1_000))
        .unwrap_or_else(|error| panic!("meal-order stockpile fixture failed: {error}"));
    let grain = deposit_lot_for_test(
        &registries,
        &mut base,
        stockpile,
        CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD),
        Mass::from_milligrams(100),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("meal-order grain fixture failed: {error}"));
    let berries = deposit_lot_for_test(
        &registries,
        &mut base,
        stockpile,
        CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
        Mass::from_milligrams(100),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("meal-order berry fixture failed: {error}"));
    let meat = deposit_lot_for_test(
        &registries,
        &mut base,
        stockpile,
        CommodityKey::new(MATERIAL_MEAT, FORM_FOOD),
        Mass::from_milligrams(100),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("meal-order meat fixture failed: {error}"));
    let mut forward = base.clone();
    let mut reverse = base;
    let forward_selection = [
        MaterialLotSelection::new(grain, Mass::from_milligrams(7)),
        MaterialLotSelection::new(berries, Mass::from_milligrams(11)),
        MaterialLotSelection::new(meat, Mass::from_milligrams(13)),
    ];
    let reverse_selection = [
        MaterialLotSelection::new(meat, Mass::from_milligrams(13)),
        MaterialLotSelection::new(berries, Mass::from_milligrams(11)),
        MaterialLotSelection::new(grain, Mass::from_milligrams(7)),
    ];

    let forward_outcome = validate_eat(&registries, &forward, stockpile, &forward_selection)
        .unwrap_or_else(|error| panic!("forward meal-order validation failed: {error}"))
        .commit(&mut forward)
        .unwrap_or_else(|error| panic!("forward meal-order commit failed: {error}"));
    let reverse_outcome = validate_eat(&registries, &reverse, stockpile, &reverse_selection)
        .unwrap_or_else(|error| panic!("reverse meal-order validation failed: {error}"))
        .commit(&mut reverse)
        .unwrap_or_else(|error| panic!("reverse meal-order commit failed: {error}"));

    assert_eq!(forward_outcome, reverse_outcome);
    assert_eq!(forward, reverse);
}

#[test]
fn meal_rejects_duplicate_lot_without_mutation() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5A70_0005));
    initialize_and_spend_reserves(&registries, &mut state);
    let stockpile = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("duplicate meal stockpile fixture failed: {error}"));
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD),
        Mass::from_milligrams(20),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("duplicate meal food fixture failed: {error}"));
    let selection = MaterialLotSelection::new(lot, Mass::from_milligrams(5));
    let before = state.clone();

    assert_eq!(
        validate_eat(&registries, &state, stockpile, &[selection, selection]),
        Err(EatError::DuplicateLot { lot })
    );
    assert_eq!(state, before);
}

#[test]
fn preservation_multiplier_extends_food_shelf_life_without_mutation() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5A70_0002));
    let berries = CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD);
    let shelf_life = registries
        .survival()
        .get_food(berries)
        .unwrap_or_else(|| panic!("berry food definition disappeared"))
        .shelf_life();
    let profile = StockpileStorageProfile::with_preservation(
        true,
        false,
        Temperature::from_millikelvin(350_000),
        3_000_000,
    )
    .unwrap_or_else(|error| panic!("preserved storage profile failed: {error}"));
    let stockpile = add_stockpile(&mut state, Mass::from_milligrams(1_000), profile)
        .unwrap_or_else(|error| panic!("preserved food stockpile failed: {error}"));
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        berries,
        Mass::from_milligrams(100),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("preserved berry lot failed: {error}"));

    assert_eq!(
        assess_food_freshness(&registries, &state, lot),
        Ok(FoodFreshness::Fresh {
            age: TickSpan::new(0),
            remaining: TickSpan::new(shelf_life.value() * 3),
        })
    );
}

#[test]
fn preservation_transfer_slows_future_spoilage_without_rewriting_prior_age() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5A70_0007));
    let ticks_per_day = registries.core().calendar().ticks_per_day();
    let ambient = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000))
        .unwrap_or_else(|error| panic!("ambient food stockpile failed: {error}"));
    let preserved_profile = StockpileStorageProfile::with_preservation(
        true,
        false,
        Temperature::from_millikelvin(350_000),
        3_000_000,
    )
    .unwrap_or_else(|error| panic!("preserved food profile failed: {error}"));
    let preserved = add_stockpile(&mut state, Mass::from_milligrams(1_000), preserved_profile)
        .unwrap_or_else(|error| panic!("preserved food stockpile failed: {error}"));
    let berries = deposit_lot_for_test(
        &registries,
        &mut state,
        ambient,
        CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
        Mass::from_milligrams(100),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("preservation-history berry fixture failed: {error}"));

    apply_clock_advance(&mut state, SimulationTick::new(ticks_per_day * 3));
    assert_eq!(
        assess_food_freshness(&registries, &state, berries),
        Ok(FoodFreshness::Fresh {
            age: TickSpan::new(ticks_per_day * 3),
            remaining: TickSpan::new(ticks_per_day),
        })
    );

    validate_material_transfer_for_test(
        &registries,
        &state,
        ambient,
        preserved,
        CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
        Mass::from_milligrams(100),
    )
    .unwrap_or_else(|error| panic!("preservation-history transfer failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("preservation-history transfer commit failed: {error}"));

    assert_eq!(
        assess_food_freshness(&registries, &state, berries),
        Ok(FoodFreshness::Fresh {
            age: TickSpan::new(ticks_per_day * 3),
            remaining: TickSpan::new(ticks_per_day * 3),
        })
    );
    apply_clock_advance(&mut state, SimulationTick::new(ticks_per_day * 6));
    assert_eq!(
        assess_food_freshness(&registries, &state, berries),
        Ok(FoodFreshness::Spoiled {
            age: TickSpan::new(ticks_per_day * 4),
        })
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("preservation-history audit failed: {error}"));
}

#[test]
fn freshness_remaining_horizon_preserves_storage_projection_phase() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5A70_0027));
    let source_profile = StockpileStorageProfile::with_preservation(
        true,
        false,
        Temperature::from_millikelvin(350_000),
        3_000_004,
    )
    .unwrap_or_else(|error| panic!("phase-aware freshness source profile failed: {error}"));
    let destination_profile = StockpileStorageProfile::with_preservation(
        true,
        false,
        Temperature::from_millikelvin(350_000),
        3_000_000,
    )
    .unwrap_or_else(|error| panic!("phase-aware freshness destination profile failed: {error}"));
    let source = add_stockpile(&mut state, Mass::from_milligrams(1_000), source_profile)
        .unwrap_or_else(|error| panic!("phase-aware freshness source failed: {error}"));
    let destination = add_stockpile(
        &mut state,
        Mass::from_milligrams(1_000),
        destination_profile,
    )
    .unwrap_or_else(|error| panic!("phase-aware freshness destination failed: {error}"));
    let berries = CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD);
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        berries,
        Mass::from_milligrams(100),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("phase-aware freshness berries failed: {error}"));
    apply_clock_advance(&mut state, SimulationTick::new(1));
    validate_material_transfer_for_test(
        &registries,
        &state,
        source,
        destination,
        berries,
        Mass::from_milligrams(100),
    )
    .unwrap_or_else(|error| panic!("phase-aware freshness relocation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("phase-aware freshness relocation commit failed: {error}"));

    let shelf_life = registries
        .survival()
        .get_food(berries)
        .unwrap_or_else(|| panic!("berry food definition disappeared"))
        .shelf_life();
    let expected_remaining = TickSpan::new(shelf_life.value() * 3 - 1);
    assert_eq!(
        assess_food_freshness(&registries, &state, lot),
        Ok(FoodFreshness::Fresh {
            age: TickSpan::new(1),
            remaining: expected_remaining,
        })
    );

    let one_before_spoilage = state
        .tick()
        .checked_add_span(
            expected_remaining
                .checked_sub(TickSpan::new(1))
                .unwrap_or_else(|| panic!("fresh food must have at least one remaining tick")),
        )
        .unwrap_or_else(|| panic!("food spoilage projection overflowed world time"));
    apply_clock_advance(&mut state, one_before_spoilage);
    assert!(matches!(
        assess_food_freshness(&registries, &state, lot),
        Ok(FoodFreshness::Fresh { remaining, .. }) if remaining == TickSpan::new(1)
    ));
    let spoilage_tick = state
        .tick()
        .checked_add_span(TickSpan::new(1))
        .unwrap_or_else(|| panic!("food spoilage tick overflowed world time"));
    apply_clock_advance(&mut state, spoilage_tick);
    assert!(matches!(
        assess_food_freshness(&registries, &state, lot),
        Ok(FoodFreshness::Spoiled { .. })
    ));
}

#[test]
fn partial_transfer_preserves_distinct_food_storage_age_cohorts() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5A70_0008));
    let ambient = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000))
        .unwrap_or_else(|error| panic!("merge-age ambient stockpile failed: {error}"));
    let preserved_profile = StockpileStorageProfile::with_preservation(
        true,
        false,
        Temperature::from_millikelvin(350_000),
        3_000_000,
    )
    .unwrap_or_else(|error| panic!("merge-age preservation profile failed: {error}"));
    let preserved = add_stockpile(&mut state, Mass::from_milligrams(1_000), preserved_profile)
        .unwrap_or_else(|error| panic!("merge-age preserved stockpile failed: {error}"));
    let commodity = CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD);
    let old_lot = deposit_lot_for_test(
        &registries,
        &mut state,
        ambient,
        commodity,
        Mass::from_milligrams(100),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("merge-age old berry fixture failed: {error}"));

    apply_clock_advance(&mut state, SimulationTick::new(60_000));
    let destination_lot = deposit_lot_for_test(
        &registries,
        &mut state,
        preserved,
        commodity,
        Mass::from_milligrams(20),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("merge-age fresh berry fixture failed: {error}"));
    apply_clock_advance(&mut state, SimulationTick::new(72_000));

    validate_material_transfer_for_test(
        &registries,
        &state,
        ambient,
        preserved,
        commodity,
        Mass::from_milligrams(10),
    )
    .unwrap_or_else(|error| panic!("merge-age partial transfer failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("merge-age partial transfer commit failed: {error}"));

    assert_eq!(
        state.inventory().get_lot(old_lot).map(|lot| lot.mass()),
        Some(Mass::from_milligrams(90))
    );
    assert_eq!(
        state
            .inventory()
            .get_lot(destination_lot)
            .map(|lot| lot.mass()),
        Some(Mass::from_milligrams(20))
    );
    assert_eq!(
        assess_food_freshness(&registries, &state, destination_lot),
        Ok(FoodFreshness::Fresh {
            age: TickSpan::new(4_000),
            remaining: TickSpan::new(276_000),
        })
    );
    let transferred_lot = state
        .inventory()
        .lot_ids(preserved)
        .find(|lot| *lot != destination_lot)
        .unwrap_or_else(|| panic!("older transferred berry cohort disappeared"));
    assert_eq!(
        state
            .inventory()
            .get_lot(transferred_lot)
            .map(|lot| lot.mass()),
        Some(Mass::from_milligrams(10))
    );
    assert_eq!(
        assess_food_freshness(&registries, &state, transferred_lot),
        Ok(FoodFreshness::Fresh {
            age: TickSpan::new(72_000),
            remaining: TickSpan::new(72_000),
        })
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("merge-age state audit failed: {error}"));
}

#[test]
fn drinking_moves_finite_water_volume_into_survival_owner() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5A70_0003));
    initialize_and_spend_reserves(&registries, &mut state);
    let store = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        Volume::from_microliters(10_000),
        FLUID_WATER,
        Volume::from_microliters(5_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("water store fixture failed: {error}"));
    let volume_before = calculate_fluid_volume_accounting(&state)
        .unwrap_or_else(|error| panic!("water pre-drink accounting failed: {error}"));
    let hydration_before = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("water fixture survival state is missing"))
        .hydration();

    let token = validate_drink(&registries, &state, store, Volume::from_microliters(625))
        .unwrap_or_else(|error| panic!("drink validation failed: {error}"));
    let outcome = token
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("drink commit failed: {error}"));

    let volume_after = calculate_fluid_volume_accounting(&state)
        .unwrap_or_else(|error| panic!("water post-drink accounting failed: {error}"));
    let hydration_at_admission = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("water admission survival state is missing"))
        .hydration();
    assert_eq!(volume_before.total(), volume_after.total());
    assert_eq!(
        volume_after.get_volume(FLUID_WATER),
        AggregateVolume::from_volume(Volume::from_microliters(5_000))
    );
    assert_eq!(
        state
            .fluid()
            .get_store(store)
            .map(|record| record.stored_volume()),
        Some(Volume::from_microliters(4_375))
    );
    assert_eq!(outcome.hydration_offered(), Volume::from_microliters(625));
    assert_eq!(hydration_at_admission, hydration_before);
    assert_eq!(finish_direct_consumption(&registries, &mut state), 1);
    let hydration_after = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("water completed survival state is missing"))
        .hydration();
    assert_eq!(
        hydration_after,
        hydration_before
            .checked_add(Volume::from_microliters(625))
            .and_then(|value| value
                .checked_sub(registries.survival().physiology().hydration_loss_per_tick()))
            .unwrap_or_else(|| panic!("hydration expectation overflowed"))
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("water post-drink audit failed: {error}"));
}
