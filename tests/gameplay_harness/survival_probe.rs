//! Bounded survival-provisioning gameplay probe over authored food, preservation, and finite water.

use deep_hearth::content::gameplay_fixture::{seed_fluid_store, seed_lot};
use deep_hearth::content::{
    FLUID_WATER, FORM_FOOD, MATERIAL_BERRIES, MATERIAL_GRAIN, MATERIAL_MEAT,
};
use deep_hearth::core::quantity::{Mass, Temperature, Volume};
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::core::time::WorldSeed;
use deep_hearth::fluid::calculate_fluid_volume_accounting;
use deep_hearth::inventory::{
    MaterialLotId, MaterialLotSelection, StockpileStorageProfile, add_stockpile,
};
use deep_hearth::material::CommodityKey;
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::registry::Registries;
use deep_hearth::simulation::advance_tick;
use deep_hearth::survival::{
    FoodCategory, FoodFreshness, assess_food_freshness, assess_survival,
    initialize_player_survival, validate_drink, validate_eat,
};

use super::seed::mix64;

const ROOM_TEMPERATURE: Temperature = Temperature::from_millikelvin(293_150);

fn mass_for_target_energy(registries: &Registries, commodity: CommodityKey, target: u128) -> Mass {
    let food = registries
        .survival()
        .get_food(commodity)
        .unwrap_or_else(|| panic!("survival probe food {} disappeared", commodity.value()));
    let per_milligram = u128::from(food.dietary_energy().nanojoules_per_milligram());
    let milligrams = target.max(1).div_ceil(per_milligram).max(1);
    let milligrams = u64::try_from(milligrams)
        .unwrap_or_else(|_| panic!("survival probe meal mass exceeds authoritative range"));
    Mass::from_milligrams(milligrams)
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
    let grain = CommodityKey::new(MATERIAL_GRAIN, FORM_FOOD);
    let berries = CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD);
    let meat = CommodityKey::new(MATERIAL_MEAT, FORM_FOOD);
    let physiology = registries.survival().physiology();

    let target_absorbed_energy = physiology.maximum_metabolic_energy().nanojoules() / 1_000;
    let category_target = (target_absorbed_energy / 3).max(1);
    let grain_mass = mass_for_target_energy(registries, grain, category_target);
    let berry_mass = mass_for_target_energy(registries, berries, category_target);
    let meat_mass = mass_for_target_energy(registries, meat, category_target);
    let meal_mass = grain_mass
        .checked_add(berry_mass)
        .and_then(|mass| mass.checked_add(meat_mass))
        .unwrap_or_else(|| panic!("survival probe meal mass overflowed"));
    let meal_mode = (mix64(seed ^ 0x4D45_414C_4348_4F49) % 4) as u8;
    let include_grain = meal_mode != 3;
    let include_berries = meal_mode != 2;
    let include_meat = meal_mode != 1;

    let preservation_multiplier_ppm =
        2_000_000 + (mix64(seed ^ 0x5052_4553_4552_5645) % 2_000_001) as u32;
    let preserved_profile = StockpileStorageProfile::with_preservation(
        true,
        false,
        Temperature::from_millikelvin(350_000),
        preservation_multiplier_ppm,
    )
    .unwrap_or_else(|error| panic!("survival probe preservation profile failed: {error}"));

    let berry_shelf_life = registries
        .survival()
        .get_food(berries)
        .unwrap_or_else(|| panic!("survival probe berry definition disappeared"))
        .shelf_life()
        .value();
    let age_limit = (berry_shelf_life / 4).max(1);
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

    let offered_food_hydration = [
        (grain, grain_mass, include_grain),
        (berries, berry_mass, include_berries),
        (meat, meat_mass, include_meat),
    ]
    .into_iter()
    .filter(|(_, _, included)| *included)
    .try_fold(0_u128, |total, (commodity, mass, _)| {
        let food = registries
            .survival()
            .get_food(commodity)
            .unwrap_or_else(|| panic!("survival probe food definition disappeared"));
        total.checked_add(
            u128::from(mass.milligrams()) * u128::from(food.hydration_microliters_per_milligram()),
        )
    })
    .unwrap_or_else(|| panic!("survival probe food hydration overflowed"));
    let hydration_deficit = hydration_loss
        .checked_mul(u128::from(depletion_ticks))
        .unwrap_or_else(|| panic!("survival probe hydration deficit overflowed"));
    let target_drink_gain = hydration_deficit
        .saturating_sub(offered_food_hydration)
        .max(2)
        / 2;
    let drink = registries
        .survival()
        .get_drink(FLUID_WATER)
        .unwrap_or_else(|| panic!("survival probe water drink definition disappeared"));
    let drink_volume = target_drink_gain
        .checked_mul(1_000_000)
        .unwrap_or_else(|| panic!("survival probe drink scaling overflowed"))
        .div_ceil(u128::from(drink.hydration_multiplier_ppm()))
        .max(1);
    let drink_volume = u64::try_from(drink_volume)
        .unwrap_or_else(|_| panic!("survival probe drink volume exceeds authoritative range"));
    let drink_volume = Volume::from_microliters(drink_volume);
    let water_capacity = drink_volume
        .checked_add(drink_volume)
        .unwrap_or_else(|| panic!("survival probe water capacity overflowed"));

    let mut state = AppState::new(WorldSeed::new(seed));
    let ambient = add_stockpile(
        &mut state,
        Mass::from_milligrams(1),
        StockpileStorageProfile::solid_only(),
    )
    .unwrap_or_else(|error| panic!("survival probe ambient storage failed: {error}"));
    let meal_storage = add_stockpile(&mut state, meal_mass, preserved_profile)
        .unwrap_or_else(|error| panic!("survival probe preserved storage failed: {error}"));
    let ambient_berries = seed_lot(
        registries,
        &mut state,
        ambient,
        berries,
        Mass::from_milligrams(1),
        ROOM_TEMPERATURE,
    );
    let grain_lot = seed_lot(
        registries,
        &mut state,
        meal_storage,
        grain,
        grain_mass,
        ROOM_TEMPERATURE,
    );
    let berry_lot = seed_lot(
        registries,
        &mut state,
        meal_storage,
        berries,
        berry_mass,
        ROOM_TEMPERATURE,
    );
    let meat_lot = seed_lot(
        registries,
        &mut state,
        meal_storage,
        meat,
        meat_mass,
        ROOM_TEMPERATURE,
    );
    let water = seed_fluid_store(
        registries,
        &mut state,
        water_capacity,
        FLUID_WATER,
        water_capacity,
        ROOM_TEMPERATURE,
    );

    advance_exact(registries, &mut state, age_ticks);
    let ambient_age = fresh_age(registries, &state, ambient_berries);
    let preserved_age = fresh_age(registries, &state, berry_lot);
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

    let mut selections = Vec::with_capacity(3);
    let mut selected_categories = Vec::with_capacity(3);
    if include_grain {
        selections.push(MaterialLotSelection::new(grain_lot, grain_mass));
        selected_categories.push(FoodCategory::Grain);
    }
    if include_berries {
        selections.push(MaterialLotSelection::new(berry_lot, berry_mass));
        selected_categories.push(FoodCategory::Fruit);
    }
    if include_meat {
        selections.push(MaterialLotSelection::new(meat_lot, meat_mass));
        selected_categories.push(FoodCategory::Protein);
    }
    let meal = validate_eat(registries, &state, meal_storage, &selections)
        .unwrap_or_else(|error| panic!("survival probe varied meal validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("survival probe varied meal commit failed: {error}"));
    assert_eq!(meal.portions().len(), selections.len());
    assert!(
        selected_categories.len() >= 2,
        "survival probe meal policy must select at least two food categories"
    );
    for category in selected_categories.iter().copied() {
        assert!(
            meal.nutrition_gained().get(category) > 0,
            "survival probe varied meal must contribute every selected food category"
        );
    }
    assert!(!meal.energy_gained().is_zero());

    let drank = validate_drink(registries, &state, water, drink_volume)
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
        "drinking must transfer finite water into survival ownership rather than delete it"
    );
    validate_loaded_state(registries, &state)
        .unwrap_or_else(|error| panic!("survival probe final persistence audit failed: {error}"));

    let category_label = selected_categories
        .iter()
        .map(|category| format!("{category:?}"))
        .collect::<Vec<_>>()
        .join("+");
    std::println!(
        "PLAYABLE SURVIVAL seed=0x{seed:016X} world-bootstrap=[food,water,storage] storage=[elapsed:{age_ticks}t preservation:{preservation_multiplier_ppm}ppm ambient_age:{ambient_age}t preserved_age:{preserved_age}t] depletion={depletion_ticks}t meal=[categories:{category_label} mass:{}mg energy:+{}nJ nutrition:+{}ppm] drink=[volume:{}uL hydration:+{}uL] matter=conserved fluid=conserved tick={}",
        meal.total_mass().milligrams(),
        meal.energy_gained().nanojoules(),
        meal.nutrition_gained().total_ppm(),
        drank.volume().microliters(),
        drank.hydration_gained().microliters(),
        state.tick().value(),
    );
}
