//! Contract tests for direct food and fluid consumption.

use super::*;
use crate::content::{
    FLUID_WATER, FORM_FOOD, FORM_LUMP, MATERIAL_BERRIES, MATERIAL_GRAIN, MATERIAL_MEAT,
    MATERIAL_STONE, PROCESS_KNAP_STONE_TOOL, build_registries,
};
use crate::core::quantity::{AggregateMass, AggregateVolume, Energy, Mass, Temperature, Volume};
use crate::core::state::{
    AppState, StateValidationError, apply_clock_advance, validate_loaded_state,
};
use crate::core::time::{SimulationTick, TickSpan, WorldSeed};
use crate::crafting::{ManualCraftStartRequest, validate_start_manual_craft};
use crate::fluid::{add_fluid_store_with_contents_for_fixture, calculate_fluid_volume_accounting};
use crate::inventory::{
    MaterialLotSelection, StockpileStorageProfile, add_solid_stockpile_for_test, add_stockpile,
    deposit_lot_for_test, validate_material_transfer_for_test,
};
use crate::labor::{PlayerWork, PlayerWorkValidationError};
use crate::material::CommodityKey;
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
use crate::registry::Registries;
use crate::simulation::advance_tick;
use crate::survival::{
    FoodCategory, NUTRITION_PARTS_PER_MILLION, NutritionReserves, Vitality, assess_survival,
    initialize_player_survival, player_record,
};

fn initialize_and_spend_reserves(registries: &Registries, state: &mut AppState) {
    initialize_player_survival(registries, state)
        .unwrap_or_else(|error| panic!("survival initialization failed: {error}"));
    for _ in 0..5 {
        let _ = advance_tick(registries, state)
            .unwrap_or_else(|error| panic!("survival reserve-spend tick failed: {error}"));
    }
}

fn finish_direct_consumption(registries: &Registries, state: &mut AppState) -> u64 {
    let active = state
        .player_work()
        .active()
        .unwrap_or_else(|| panic!("direct-consumption test has no active player work"));
    let completes_at = match active {
        PlayerWork::Eating { work } => work.completes_at(),
        PlayerWork::Drinking { work } => work.completes_at(),
        other @ (PlayerWork::ManualProduction { .. }
        | PlayerWork::Mining { .. }
        | PlayerWork::ManualPower { .. }
        | PlayerWork::Prospecting { .. }) => {
            panic!("direct-consumption test has wrong active work: {other:?}")
        }
    };
    let started = state.tick().value();
    while state.tick() < completes_at {
        let _ = advance_tick(registries, state)
            .unwrap_or_else(|error| panic!("direct-consumption completion tick failed: {error}"));
    }
    assert_eq!(state.player_work().active(), None);
    completes_at.value() - started
}

#[test]
fn death_during_drinking_releases_attention_and_discards_unabsorbed_intake_next_tick() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5A70_0026));
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
    assert!(matches!(
        state.player_work().active(),
        Some(PlayerWork::Drinking { .. })
    ));
    assert!(state.survival().pending_direct_consumption().is_some());
    let hydration_at_death = state
        .survival()
        .player()
        .unwrap_or_else(|| panic!("death-during-drink player disappeared"))
        .hydration();
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("death-during-drink fatal state failed audit: {error}"));

    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("death-during-drink cleanup tick failed: {error}"));
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

#[test]
fn direct_consumption_claims_quantity_scaled_player_attention() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5A70_001B));
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
    let water = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        Volume::from_microliters(1_000),
        FLUID_WATER,
        Volume::from_microliters(1_000),
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
        Some(EatError::PlayerBusy { active })
    );
    assert_eq!(
        validate_drink(&registries, &state, water, Volume::from_microliters(100)).err(),
        Some(DrinkError::PlayerBusy { active })
    );
    assert_eq!(state, before_rejected_actions);

    let duration = work.completes_at().value() - state.tick().value();
    for _ in 0..duration {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("attention meal tick failed: {error}"));
    }
    assert_eq!(state.player_work().active(), None);
    assert!(
        validate_drink(&registries, &state, water, Volume::from_microliters(100)).is_ok(),
        "direct drinking must become available after the authored meal interval finishes"
    );
}

#[test]
fn drinking_rejects_volume_above_authored_intake_limit_without_consumption() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5A70_001A));
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
    assert_eq!(
        outcome.energy_offered(),
        Energy::from_nanojoules(14_000_000_000)
    );
    assert_eq!(outcome.hydration_offered(), AggregateVolume::ZERO);
    assert_eq!(outcome.nutrition_offered().total_ppm(), 0);
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
    );
}

#[test]
fn meal_energy_first_covers_same_tick_metabolic_shortfall() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5A70_0024));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("meal-shortfall survival setup failed: {error}"));
    let physiology = registries.survival().physiology();
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
    let stockpile = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1))
        .unwrap_or_else(|error| panic!("meal-shortfall stockpile failed: {error}"));
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD),
        Mass::from_milligrams(1),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("meal-shortfall food lot failed: {error}"));
    let outcome = validate_eat(
        &registries,
        &state,
        stockpile,
        &[MaterialLotSelection::new(lot, Mass::from_milligrams(1))],
    )
    .unwrap_or_else(|error| panic!("meal-shortfall validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("meal-shortfall commit failed: {error}"));
    assert_eq!(
        outcome.energy_offered(),
        Energy::from_nanojoules(14_000_000_000)
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
fn validated_eat_rejects_survival_change_before_commit_without_mutation() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5A70_001C));
    initialize_and_spend_reserves(&registries, &mut state);
    let stockpile = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(10))
        .unwrap_or_else(|error| panic!("stale-survival meal stockpile failed: {error}"));
    let food = deposit_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD),
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("stale-survival meal fixture failed: {error}"));
    let token = validate_eat(
        &registries,
        &state,
        stockpile,
        &[MaterialLotSelection::new(food, Mass::from_milligrams(1))],
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
    let mut state = AppState::new(WorldSeed::new(0x5A70_001D));
    initialize_and_spend_reserves(&registries, &mut state);
    let stockpile = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(10))
        .unwrap_or_else(|error| panic!("stale-inventory meal stockpile failed: {error}"));
    let food = deposit_lot_for_test(
        &registries,
        &mut state,
        stockpile,
        CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD),
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("stale-inventory meal fixture failed: {error}"));
    let token = validate_eat(
        &registries,
        &state,
        stockpile,
        &[MaterialLotSelection::new(food, Mass::from_milligrams(1))],
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
    let mut state = AppState::new(WorldSeed::new(0x5A70_001E));
    initialize_and_spend_reserves(&registries, &mut state);
    let water = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        Volume::from_microliters(10),
        FLUID_WATER,
        Volume::from_microliters(10),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("stale-survival drink fixture failed: {error}"));
    let token = validate_drink(&registries, &state, water, Volume::from_microliters(1))
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
    let mut state = AppState::new(WorldSeed::new(0x5A70_001F));
    initialize_and_spend_reserves(&registries, &mut state);
    let water = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        Volume::from_microliters(10),
        FLUID_WATER,
        Volume::from_microliters(10),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("stale-fluid drink fixture failed: {error}"));
    let token = validate_drink(&registries, &state, water, Volume::from_microliters(1))
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

#[test]
fn trusted_load_replays_direct_consumption_attention_durations() {
    let registries = build_registries();

    let mut eating = AppState::new(WorldSeed::new(0x5A70_0020));
    initialize_and_spend_reserves(&registries, &mut eating);
    let stockpile = add_solid_stockpile_for_test(&mut eating, Mass::from_milligrams(10))
        .unwrap_or_else(|error| panic!("eating-duration stockpile failed: {error}"));
    let food = deposit_lot_for_test(
        &registries,
        &mut eating,
        stockpile,
        CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD),
        Mass::from_milligrams(10),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("eating-duration food fixture failed: {error}"));
    let _eating_outcome = validate_eat(
        &registries,
        &eating,
        stockpile,
        &[MaterialLotSelection::new(food, Mass::from_milligrams(1))],
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

    let mut drinking = AppState::new(WorldSeed::new(0x5A70_0021));
    initialize_and_spend_reserves(&registries, &mut drinking);
    let water = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut drinking,
        Volume::from_microliters(10),
        FLUID_WATER,
        Volume::from_microliters(10),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("drinking-duration water fixture failed: {error}"));
    let _drinking_outcome =
        validate_drink(&registries, &drinking, water, Volume::from_microliters(1))
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
    let mut state = AppState::new(WorldSeed::new(0x5A70_0022));
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
    let volume = Volume::from_microliters(125_000);
    assert_eq!(
        physiology.direct_consumption().drink_duration(volume),
        Some(TickSpan::new(3))
    );
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
        .checked_add(Volume::from_microliters(41_666))
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

    for _ in 0..2 {
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
                    physiology.hydration_loss_per_tick().microliters() * 3,
                ))
            })
            .unwrap_or_else(|| panic!("multi-tick final hydration expectation failed"))
    );
}

#[test]
fn dead_player_pending_consumption_cancels_on_next_tick_with_player_work() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5A70_0026));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("dead-consumption survival setup failed: {error}"));
    let volume = Volume::from_microliters(125_000);
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
    let mut state = AppState::new(WorldSeed::new(0x5A70_0023));
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
        .unwrap_or_else(|error| panic!("partial-hydration drink validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("partial-hydration drink commit failed: {error}"));

    assert_eq!(outcome.volume(), Volume::from_microliters(10));
    assert_eq!(outcome.hydration_offered(), Volume::from_microliters(10));
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
    assert_eq!(finish_direct_consumption(&registries, &mut state), 1);
    assert_eq!(
        assess_survival(&registries, &state)
            .unwrap_or_else(|| panic!("partial-hydration survival state disappeared after drink"))
            .hydration(),
        hydration_before
            .checked_sub(physiology.hydration_loss_per_tick())
            .and_then(|value| value.checked_add(outcome.hydration_offered()))
            .unwrap_or_else(|| panic!("partial-hydration expected reserve underflowed"))
    );
}

#[test]
fn drink_hydration_first_covers_same_tick_hydration_shortfall() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5A70_0025));
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
    let volume = Volume::from_microliters(10);
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
    assert_eq!(finish_direct_consumption(&registries, &mut state), 1);
    let shortfall = physiology
        .hydration_loss_per_tick()
        .checked_sub(hydration_before)
        .unwrap_or(Volume::ZERO);
    let expected_after = outcome
        .hydration_offered()
        .checked_sub(shortfall)
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
    let mut state = AppState::new(WorldSeed::new(0x5A70_0010));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("full-reserve survival initialization failed: {error}"));
    let physiology = registries.survival().physiology();
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
    let outcome = validate_eat(
        &registries,
        &state,
        stockpile,
        &[MaterialLotSelection::new(lot, Mass::from_milligrams(1))],
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

    assert_eq!(
        outcome.energy_offered(),
        Energy::from_nanojoules(1_400_000_000_000)
    );
    assert_eq!(outcome.nutrition_offered().get(FoodCategory::Grain), 70);
    assert_eq!(
        assess_survival(&registries, &state)
            .unwrap_or_else(|| panic!("nutrition-clamp survival state disappeared at admission"))
            .nutrition()
            .get(FoodCategory::Grain),
        0
    );
    assert_eq!(finish_direct_consumption(&registries, &mut state), 1);
    assert_eq!(
        assess_survival(&registries, &state)
            .unwrap_or_else(|| panic!("nutrition-clamp survival state disappeared"))
            .nutrition()
            .get(FoodCategory::Grain),
        65
    );
}

#[test]
fn very_large_meal_is_rejected_by_authored_intake_limit_without_consumption() {
    const MEAL_MASS_MG: u64 = 7_000_000_000;

    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x5A70_0019));
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
    let mut state = AppState::new(WorldSeed::new(0x5A70_0011));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("full-hydration survival initialization failed: {error}"));
    let physiology = registries.survival().physiology();
    let volume = Volume::from_microliters(1_000);
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
    assert_eq!(finish_direct_consumption(&registries, &mut state), 1);
    assert_eq!(
        assess_survival(&registries, &state)
            .unwrap_or_else(|| panic!("full-hydration player disappeared after drinking"))
            .hydration(),
        physiology.maximum_hydration()
    );
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

    let one_before_spoilage =
        SimulationTick::new(state.tick().value() + expected_remaining.value() - 1);
    apply_clock_advance(&mut state, one_before_spoilage);
    assert!(matches!(
        assess_food_freshness(&registries, &state, lot),
        Ok(FoodFreshness::Fresh { remaining, .. }) if remaining == TickSpan::new(1)
    ));
    let spoilage_tick = SimulationTick::new(state.tick().value() + 1);
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
