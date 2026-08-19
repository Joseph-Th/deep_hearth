//! Bounded survival-provisioning gameplay probe over authored food, preservation, and finite drink.

use std::collections::{BTreeMap, BTreeSet};

use deep_hearth::content::gameplay_fixture::{seed_fluid_store, seed_lot};
use deep_hearth::core::quantity::{Mass, Temperature, Volume};
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::core::time::WorldSeed;
use deep_hearth::fluid::calculate_fluid_volume_accounting;
use deep_hearth::inventory::{
    MaterialLotId, MaterialLotSelection, StockpileStorageProfile, add_stockpile,
};
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::registry::Registries;
use deep_hearth::simulation::advance_tick;
use deep_hearth::survival::{
    FoodCategory, FoodDefinition, FoodFreshness, assess_food_freshness, assess_survival,
    initialize_player_survival, validate_drink, validate_eat,
};

use super::seed::mix64;

const ROOM_TEMPERATURE: Temperature = Temperature::from_millikelvin(293_150);

fn mass_for_target_energy(food: FoodDefinition, target: u128) -> Mass {
    let per_milligram = u128::from(food.dietary_energy().nanojoules_per_milligram());
    let milligrams = target.max(1).div_ceil(per_milligram).max(1);
    let milligrams = u64::try_from(milligrams)
        .unwrap_or_else(|_| panic!("survival probe meal mass exceeds authoritative range"));
    Mass::from_milligrams(milligrams)
}

const fn category_salt(category: FoodCategory) -> u64 {
    match category {
        FoodCategory::Grain => 0x4752_4149_4E00_0001,
        FoodCategory::Fruit => 0x4652_5549_5400_0002,
        FoodCategory::Protein => 0x5052_4F54_4549_4E03,
    }
}

fn fresh_age(registries: &Registries, state: &AppState, lot: MaterialLotId) -> u64 {
    match assess_food_freshness(registries, state, lot)
        .unwrap_or_else(|error| panic!("survival probe freshness projection failed: {error:?}"))
    {
        FoodFreshness::Fresh { age, remaining: _ } => age.value(),
        FoodFreshness::Spoiled { age } => {
            panic!(
                "survival probe bounded setup unexpectedly spoiled food at age {} ticks",
                age.value()
            )
        }
    }
}

fn advance_exact(registries: &Registries, state: &mut AppState, ticks: u64) {
    for _ in 0..ticks {
        advance_tick(registries, state)
            .unwrap_or_else(|error| panic!("survival probe tick failed: {error}"));
    }
}

pub(super) fn run_survival_provisioning_probe(registries: &Registries, seed: u64) {
    let physiology = registries.survival().physiology();

    let mut foods_by_category = BTreeMap::<FoodCategory, Vec<FoodDefinition>>::new();
    for food in registries.survival().foods().copied() {
        foods_by_category
            .entry(food.category())
            .or_default()
            .push(food);
    }
    assert!(
        !foods_by_category.is_empty(),
        "survival gameplay is stale or unavailable: the runtime registry has no authored edible food"
    );
    let authored_categories = foods_by_category.keys().copied().collect::<Vec<_>>();
    let selected_category_count = if authored_categories.len() >= 3 {
        2 + usize::from(!mix64(seed ^ 0x4D45_414C_4348_4F49).is_multiple_of(2))
    } else {
        authored_categories.len()
    };
    let category_start =
        usize::try_from(mix64(seed ^ 0x4341_5445_474F_5259) % authored_categories.len() as u64)
            .unwrap_or_else(|_| unreachable!("food category index fits usize"));
    let selected_categories = (0..selected_category_count)
        .map(|offset| authored_categories[(category_start + offset) % authored_categories.len()])
        .collect::<Vec<_>>();

    let target_absorbed_energy = physiology.maximum_metabolic_energy().nanojoules() / 1_000;
    let category_target = (target_absorbed_energy / selected_category_count as u128).max(1);
    let offerings = selected_categories
        .iter()
        .copied()
        .enumerate()
        .map(|(index, category)| {
            let options = foods_by_category
                .get(&category)
                .unwrap_or_else(|| unreachable!("selected authored food category disappeared"));
            let choice = usize::try_from(
                mix64(seed ^ category_salt(category) ^ index as u64) % options.len() as u64,
            )
            .unwrap_or_else(|_| unreachable!("food option index fits usize"));
            let food = options[choice];
            (food, mass_for_target_energy(food, category_target))
        })
        .collect::<Vec<_>>();
    let meal_mass = offerings
        .iter()
        .try_fold(Mass::ZERO, |total, (_, mass)| total.checked_add(*mass))
        .unwrap_or_else(|| panic!("survival probe meal mass overflowed"));
    let witness_index =
        usize::try_from(mix64(seed ^ 0x5052_4553_5749_544E) % offerings.len() as u64)
            .unwrap_or_else(|_| unreachable!("preservation witness index fits usize"));
    let witness_food = offerings[witness_index].0;

    let preservation_multiplier_ppm =
        2_000_000 + (mix64(seed ^ 0x5052_4553_4552_5645) % 2_000_001) as u32;
    let preserved_profile = StockpileStorageProfile::with_preservation(
        true,
        false,
        Temperature::from_millikelvin(350_000),
        preservation_multiplier_ppm,
    )
    .unwrap_or_else(|error| panic!("survival probe preservation profile failed: {error}"));

    let age_limit = (witness_food.shelf_life().value() / 4).max(1);
    let age_ticks = (256 + mix64(seed ^ 0x4147_455F_464F_4F44) % 512).min(age_limit);

    let basal_energy = physiology.basal_energy_cost_per_tick().nanojoules();
    let hydration_loss = u128::from(physiology.hydration_loss_per_tick().microliters());
    let energy_ticks = target_absorbed_energy.div_ceil(basal_energy);
    let target_hydration_deficit = u128::from(physiology.maximum_hydration().microliters()) / 100;
    let hydration_ticks = target_hydration_deficit.div_ceil(hydration_loss);
    let varied_ticks = u128::from(128 + mix64(seed ^ 0x4445_504C_4554_4552) % 128);
    let depletion_ticks = energy_ticks.max(hydration_ticks).max(varied_ticks);
    let depletion_ticks = u64::try_from(depletion_ticks)
        .unwrap_or_else(|_| panic!("survival probe depletion horizon exceeds tick range"));

    let offered_food_hydration = offerings
        .iter()
        .try_fold(0_u128, |total, (food, mass)| {
            let contribution = u128::from(mass.milligrams())
                .checked_mul(u128::from(food.hydration_microliters_per_milligram()))?;
            total.checked_add(contribution)
        })
        .unwrap_or_else(|| panic!("survival probe food hydration overflowed"));
    let hydration_deficit = hydration_loss
        .checked_mul(u128::from(depletion_ticks))
        .unwrap_or_else(|| panic!("survival probe hydration deficit overflowed"));
    let target_drink_gain = hydration_deficit
        .saturating_sub(offered_food_hydration)
        .max(2)
        / 2;
    let drinks = registries.survival().drinks().copied().collect::<Vec<_>>();
    assert!(
        !drinks.is_empty(),
        "survival gameplay is stale or unavailable: the runtime registry has no authored drinkable fluid"
    );
    let drink_index = usize::try_from(mix64(seed ^ 0x4452_494E_4B00_0001) % drinks.len() as u64)
        .unwrap_or_else(|_| unreachable!("drink index fits usize"));
    let drink = drinks[drink_index];
    let drink_fluid = drink.fluid();
    let drink_volume = target_drink_gain
        .checked_mul(1_000_000)
        .unwrap_or_else(|| panic!("survival probe drink scaling overflowed"))
        .div_ceil(u128::from(drink.hydration_multiplier_ppm()))
        .max(1);
    let drink_volume = u64::try_from(drink_volume)
        .unwrap_or_else(|_| panic!("survival probe drink volume exceeds authoritative range"));
    let drink_volume = Volume::from_microliters(drink_volume);
    let drink_capacity = drink_volume
        .checked_add(drink_volume)
        .unwrap_or_else(|| panic!("survival probe drink capacity overflowed"));

    let mut state = AppState::new(WorldSeed::new(seed));
    let ambient = add_stockpile(
        &mut state,
        Mass::from_milligrams(1),
        StockpileStorageProfile::solid_only(),
    )
    .unwrap_or_else(|error| panic!("survival probe ambient storage failed: {error}"));
    let meal_storage = add_stockpile(&mut state, meal_mass, preserved_profile)
        .unwrap_or_else(|error| panic!("survival probe preserved storage failed: {error}"));
    let ambient_witness = seed_lot(
        registries,
        &mut state,
        ambient,
        witness_food.commodity(),
        Mass::from_milligrams(1),
        ROOM_TEMPERATURE,
    );
    let prepared = offerings
        .iter()
        .map(|(food, mass)| {
            let lot = seed_lot(
                registries,
                &mut state,
                meal_storage,
                food.commodity(),
                *mass,
                ROOM_TEMPERATURE,
            );
            (*food, *mass, lot)
        })
        .collect::<Vec<_>>();
    let preserved_witness = prepared[witness_index].2;
    let drink_store = seed_fluid_store(
        registries,
        &mut state,
        drink_capacity,
        drink_fluid,
        drink_capacity,
        ROOM_TEMPERATURE,
    );

    advance_exact(registries, &mut state, age_ticks);
    let ambient_age = fresh_age(registries, &state, ambient_witness);
    let preserved_age = fresh_age(registries, &state, preserved_witness);
    assert!(
        preserved_age < ambient_age,
        "authored preservation must slow future food spoilage relative to ambient storage"
    );

    initialize_player_survival(registries, &mut state)
        .unwrap_or_else(|error| panic!("survival probe player initialization failed: {error}"));
    advance_exact(registries, &mut state, depletion_ticks);
    let before = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("survival probe player disappeared before provisioning"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("survival probe initial matter audit failed: {error}"));
    let fluid_before = calculate_fluid_volume_accounting(&state)
        .unwrap_or_else(|error| panic!("survival probe initial fluid audit failed: {error}"));

    let selections = prepared
        .iter()
        .map(|(_, mass, lot)| MaterialLotSelection::new(*lot, *mass))
        .collect::<Vec<_>>();
    let selected_categories = prepared
        .iter()
        .map(|(food, _, _)| food.category())
        .collect::<Vec<_>>();
    let meal = validate_eat(registries, &state, meal_storage, &selections)
        .unwrap_or_else(|error| panic!("survival probe varied meal validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("survival probe varied meal commit failed: {error}"));
    assert_eq!(meal.portions().len(), selections.len());
    assert_eq!(
        selected_categories
            .iter()
            .copied()
            .collect::<BTreeSet<_>>()
            .len(),
        selected_categories.len(),
        "survival probe meal selection must not duplicate a dietary category"
    );
    assert!(
        selected_categories.len() >= authored_categories.len().min(2),
        "survival probe meal policy must select multiple authored food categories when the registry provides them"
    );
    for category in selected_categories.iter().copied() {
        assert!(
            meal.nutrition_gained().get(category) > 0,
            "survival probe varied meal must contribute every selected food category"
        );
    }
    assert!(!meal.energy_gained().is_zero());

    let drank = validate_drink(registries, &state, drink_store, drink_volume)
        .unwrap_or_else(|error| panic!("survival probe drinking validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("survival probe drinking commit failed: {error}"));
    assert!(!drank.hydration_gained().is_zero());

    let after = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("survival probe player disappeared after provisioning"));
    assert!(after.metabolic_energy() > before.metabolic_energy());
    assert!(after.hydration() > before.hydration());
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("survival probe final matter audit failed: {error}"))
            .total(),
        matter_before.total(),
        "eating must transfer matter into survival ownership rather than delete it"
    );
    assert_eq!(
        calculate_fluid_volume_accounting(&state)
            .unwrap_or_else(|error| panic!("survival probe final fluid audit failed: {error}"))
            .total(),
        fluid_before.total(),
        "drinking must transfer finite fluid into survival ownership rather than delete it"
    );
    validate_loaded_state(registries, &state)
        .unwrap_or_else(|error| panic!("survival probe final persistence audit failed: {error}"));

    let category_label = selected_categories
        .iter()
        .map(|category| format!("{category:?}"))
        .collect::<Vec<_>>()
        .join("+");
    let food_label = prepared
        .iter()
        .map(|(food, _, _)| food.commodity().value().to_string())
        .collect::<Vec<_>>()
        .join("+");
    std::println!(
        "PLAYABLE SURVIVAL seed=0x{seed:016X} catalog=registry-derived world-bootstrap=[authored-food,authored-drink,storage-profile] storage=[witness:{} elapsed:{age_ticks}t preservation:{preservation_multiplier_ppm}ppm ambient_age:{ambient_age}t preserved_age:{preserved_age}t] depletion={depletion_ticks}t meal=[foods:{food_label} categories:{category_label} mass:{}mg energy:+{}nJ nutrition:+{}ppm] drink=[fluid:{} volume:{}uL hydration:+{}uL] matter=conserved fluid=conserved tick={}",
        witness_food.commodity().value(),
        meal.total_mass().milligrams(),
        meal.energy_gained().nanojoules(),
        meal.nutrition_gained().total_ppm(),
        drink_fluid.value(),
        drank.volume().microliters(),
        drank.hydration_gained().microliters(),
        state.tick().value(),
    );
}
