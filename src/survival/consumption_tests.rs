//! Tests for the sibling consumption module; isolated so test-only edits do not invalidate production builds.

use super::*;
use crate::content::{
    FLUID_WATER, FORM_FOOD, FORM_LUMP, MATERIAL_BERRIES, MATERIAL_GRAIN, MATERIAL_MEAT,
    MATERIAL_STONE, PROCESS_KNAP_STONE_TOOL, build_registries,
};
use crate::core::quantity::{AggregateMass, AggregateVolume, Temperature};
use crate::core::state::{apply_clock_advance, validate_loaded_state};
use crate::core::time::{SimulationTick, WorldSeed};
use crate::crafting::{ManualCraftStartRequest, validate_start_manual_craft};
use crate::fluid::{add_fluid_store_with_contents_for_fixture, calculate_fluid_volume_accounting};
use crate::inventory::{
    StockpileStorageProfile, add_solid_stockpile_for_test, add_stockpile, deposit_lot_for_test,
    validate_material_transfer_for_test,
};
use crate::labor::PlayerWork;
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::simulation::advance_tick;
use crate::survival::assess_survival;
use crate::survival::{
    NUTRITION_PARTS_PER_MILLION, NutritionReserves, Vitality, initialize_player_survival,
};

fn initialize_and_spend_reserves(registries: &Registries, state: &mut AppState) {
    initialize_player_survival(registries, state)
        .unwrap_or_else(|error| panic!("survival initialization failed: {error}"));
    for _ in 0..5 {
        advance_tick(registries, state)
            .unwrap_or_else(|error| panic!("survival reserve-spend tick failed: {error}"));
    }
}

#[test]
fn direct_consumption_rejects_unsafe_food_and_water_temperatures_without_mutation() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5A70_0018));
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
    let water = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        Volume::from_microliters(10),
        FLUID_WATER,
        Volume::from_microliters(10),
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
        validate_drink(&registries, &state, water, Volume::from_microliters(1)).err(),
        Some(DrinkError::TemperatureOutsideConsumptionRange {
            store: water,
            temperature: hot_temperature,
            minimum: Temperature::from_millikelvin(273_150),
            maximum: Temperature::from_millikelvin(333_150),
        })
    );
    assert_eq!(state, before);
}

fn start_attention_owning_craft(registries: &Registries, state: &mut AppState) -> PlayerWork {
    let source = add_solid_stockpile_for_test(state, Mass::from_milligrams(1_000_000))
        .unwrap_or_else(|error| panic!("attention craft source fixture failed: {error}"));
    let destination = add_solid_stockpile_for_test(state, Mass::from_milligrams(1_000_000))
        .unwrap_or_else(|error| panic!("attention craft destination fixture failed: {error}"));
    deposit_lot_for_test(
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
        ManualCraftStartRequest::single(PROCESS_KNAP_STONE_TOOL, source, destination),
    )
    .unwrap_or_else(|error| panic!("attention craft validation failed: {error}"))
    .commit(state)
    .unwrap_or_else(|error| panic!("attention craft commit failed: {error}"));
    state
        .player_work()
        .active()
        .unwrap_or_else(|| panic!("attention craft did not claim player work"))
}

#[test]
fn eating_and_drinking_reject_active_player_work_without_mutation() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5A70_0012));
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
    let water = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        Volume::from_microliters(1),
        FLUID_WATER,
        Volume::from_microliters(1),
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
        Some(EatError::PlayerBusy { active })
    );
    assert_eq!(
        validate_drink(&registries, &state, water, Volume::from_microliters(1)).err(),
        Some(DrinkError::PlayerBusy { active })
    );
    assert_eq!(state, before);
}

#[test]
fn eating_with_any_reserve_room_consumes_the_exact_selected_portion() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5A70_0015));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("partial-reserve survival initialization failed: {error}"));
    let physiology = registries.survival().physiology();
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
    let stockpile = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1))
        .unwrap_or_else(|error| panic!("partial-reserve food stockpile failed: {error}"));
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD),
        Mass::from_milligrams(1),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("partial-reserve food lot failed: {error}"));

    let outcome = validate_eat(
        &registries,
        &state,
        stockpile,
        &[MaterialLotSelection::new(lot, Mass::from_milligrams(1))],
    )
    .unwrap_or_else(|error| panic!("partial-reserve eating validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("partial-reserve eating commit failed: {error}"));

    assert_eq!(outcome.total_mass(), Mass::from_milligrams(1));
    assert_eq!(outcome.energy_gained(), Energy::from_nanojoules(1));
    assert_eq!(outcome.hydration_gained(), Volume::ZERO);
    assert_eq!(outcome.nutrition_gained().total_ppm(), 0);
    assert_eq!(state.inventory().get_lot(lot), None);
    assert_eq!(
        assess_survival(&registries, &state)
            .unwrap_or_else(|| panic!("partial-reserve survival state disappeared"))
            .metabolic_energy(),
        physiology.maximum_metabolic_energy()
    );
}

#[test]
fn eating_rejects_over_capacity_hydration_without_normalizing_or_consuming_food() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5A70_0017));
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
    let mut state = AppState::new(WorldSeed::new(0x5A70_0013));
    initialize_and_spend_reserves(&registries, &mut state);
    let water = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        Volume::from_microliters(1),
        FLUID_WATER,
        Volume::from_microliters(1),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("stale-attention water fixture failed: {error}"));
    let drink = validate_drink(&registries, &state, water, Volume::from_microliters(1))
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
fn drinking_clamps_hydration_gain_while_consuming_exact_requested_volume() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5A70_0016));
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
    let store = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        Volume::from_microliters(10),
        FLUID_WATER,
        Volume::from_microliters(10),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("partial-hydration water fixture failed: {error}"));

    let outcome = validate_drink(&registries, &state, store, Volume::from_microliters(10))
        .unwrap_or_else(|error| panic!("partial-hydration drinking validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("partial-hydration drinking commit failed: {error}"));

    assert_eq!(outcome.volume(), Volume::from_microliters(10));
    assert_eq!(outcome.hydration_gained(), Volume::from_microliters(1));
    assert_eq!(
        assess_survival(&registries, &state)
            .unwrap_or_else(|| panic!("partial-hydration player disappeared"))
            .hydration(),
        physiology.maximum_hydration()
    );
    assert_eq!(
        state
            .fluid()
            .get_store(store)
            .and_then(|record| record.contents())
            .map(|contents| contents.volume()),
        None
    );
    assert_eq!(
        state.survival().consumed_fluid_volume(FLUID_WATER),
        AggregateVolume::from_microliters(10)
    );
}

#[test]
fn eating_at_full_reserves_is_rejected_without_consuming_food() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5A70_0010));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("full-reserve survival initialization failed: {error}"));
    let stockpile = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1))
        .unwrap_or_else(|error| panic!("full-reserve food stockpile failed: {error}"));
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD),
        Mass::from_milligrams(1),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("full-reserve food lot failed: {error}"));
    let before = state.clone();

    assert_eq!(
        validate_eat(
            &registries,
            &state,
            stockpile,
            &[MaterialLotSelection::new(lot, Mass::from_milligrams(1))],
        )
        .err(),
        Some(EatError::NoReserveGain {
            mass: Mass::from_milligrams(1),
        })
    );
    assert_eq!(state, before);
}

#[test]
fn nutrition_credit_uses_consumed_food_even_when_metabolic_reserve_is_full() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5A70_0014));
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
    let stockpile = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(100))
        .unwrap_or_else(|error| panic!("nutrition-clamp stockpile failed: {error}"));
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD),
        Mass::from_milligrams(100),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("nutrition-clamp food lot failed: {error}"));

    let outcome = validate_eat(
        &registries,
        &state,
        stockpile,
        &[MaterialLotSelection::new(lot, Mass::from_milligrams(100))],
    )
    .unwrap_or_else(|error| panic!("nutrition-clamp eating validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("nutrition-clamp eating commit failed: {error}"));

    assert_eq!(outcome.energy_gained(), Energy::ZERO);
    assert_eq!(outcome.nutrition_gained().get(FoodCategory::Grain), 70);
    assert_eq!(
        assess_survival(&registries, &state)
            .unwrap_or_else(|| panic!("nutrition-clamp survival state disappeared"))
            .nutrition()
            .get(FoodCategory::Grain),
        70
    );
}

#[test]
fn nutrition_normalization_handles_full_width_energy_without_scaled_overflow() {
    let maximum = Energy::from_nanojoules(u128::MAX);
    assert_eq!(
        normalized_nutrition_gain_ppm(Energy::from_nanojoules(u128::MAX), maximum),
        Ok(NUTRITION_PARTS_PER_MILLION)
    );
    assert_eq!(
        normalized_nutrition_gain_ppm(
            Energy::from_nanojoules(10_000_000_000_000_000),
            Energy::from_nanojoules(20_000_000_000_000_000),
        ),
        Ok(500_000)
    );
}

#[test]
fn nutrition_allocation_handles_full_width_energy_without_intermediate_overflow() {
    let offered = NutritionEnergy {
        grain: u128::MAX - 2,
        fruit: 1,
        protein: 1,
    };

    let gain = allocate_nutrition(NUTRITION_PARTS_PER_MILLION, offered);

    assert_eq!(gain.total_ppm(), NUTRITION_PARTS_PER_MILLION);
    assert_eq!(gain.get(FoodCategory::Grain), NUTRITION_PARTS_PER_MILLION);
    assert_eq!(gain.get(FoodCategory::Fruit), 0);
    assert_eq!(gain.get(FoodCategory::Protein), 0);
}

#[test]
fn drinking_at_full_hydration_is_rejected_without_consuming_water() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5A70_0011));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("full-hydration survival initialization failed: {error}"));
    let store = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        Volume::from_microliters(1),
        FLUID_WATER,
        Volume::from_microliters(1),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("full-hydration water fixture failed: {error}"));
    let before = state.clone();

    assert_eq!(
        validate_drink(&registries, &state, store, Volume::from_microliters(1)).err(),
        Some(DrinkError::NoHydrationGain {
            volume: Volume::from_microliters(1),
        })
    );
    assert_eq!(state, before);
}

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
    let survival_after = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("food post-consumption survival state is missing"));
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
    assert!(outcome.nutrition_gained().total_ppm() > 0);
    assert!(survival_after.metabolic_energy() > survival_before.metabolic_energy());
    assert_eq!(
        survival_after.nutrition().get(FoodCategory::Grain),
        NUTRITION_PARTS_PER_MILLION
    );
    assert_eq!(
        survival_after.nutrition().get(FoodCategory::Fruit),
        survival_before.nutrition().get(FoodCategory::Fruit)
    );
    assert_eq!(
        survival_after.nutrition().get(FoodCategory::Protein),
        survival_before.nutrition().get(FoodCategory::Protein)
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
        assert!(outcome.nutrition_gained().get(category) > 0);
    }
    let after = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("varied meal survival state disappeared"));
    assert!(
        after.nutrition().get(FoodCategory::Grain) > before.nutrition().get(FoodCategory::Grain)
    );
    assert!(
        after.nutrition().get(FoodCategory::Fruit) > before.nutrition().get(FoodCategory::Fruit)
    );
    assert!(
        after.nutrition().get(FoodCategory::Protein)
            > before.nutrition().get(FoodCategory::Protein)
    );
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
    let hydration_after = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("water post-drink survival state is missing"))
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
    assert_eq!(outcome.hydration_gained(), Volume::from_microliters(625));
    assert_eq!(
        hydration_after,
        hydration_before
            .checked_add(Volume::from_microliters(625))
            .unwrap_or_else(|| panic!("hydration expectation overflowed"))
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("water post-drink audit failed: {error}"));
}
