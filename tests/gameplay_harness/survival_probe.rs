//! Bounded survival-provisioning gameplay probe over authored food, preservation, and finite drink.

use std::collections::{BTreeMap, BTreeSet};

use deep_hearth::content::gameplay_fixture::{seed_fluid_store, seed_lot, seed_stockpile};
use deep_hearth::core::quantity::{Mass, Temperature, Volume};
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::core::time::WorldSeed;
use deep_hearth::fluid::calculate_fluid_volume_accounting;
use deep_hearth::inventory::{MaterialLotId, MaterialLotSelection, StockpileStorageProfile};
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ProvisioningPriority {
    MetabolicEnergy,
    Hydration,
    Balanced,
}

impl ProvisioningPriority {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::MetabolicEnergy => "energy",
            Self::Hydration => "hydration",
            Self::Balanced => "balanced",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SurvivalProvisioningReview {
    pub(super) preservation_age_saved_ticks: u64,
    pub(super) selected_category_count: usize,
    pub(super) authored_category_count: usize,
    pub(super) provisioning_priority: ProvisioningPriority,
    pub(super) energy_deficit_ppm: u32,
    pub(super) hydration_deficit_ppm: u32,
    pub(super) diet_quality_gain_ppm: u32,
    pub(super) retained_preserved_mass_mg: u64,
    pub(super) reserve_recovered: bool,
}

pub(super) fn evaluate_survival_provisioning_probe(
    registries: &Registries,
    seed: u64,
) -> SurvivalProvisioningReview {
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
    let provisioning_wait_ticks = age_ticks
        .checked_add(depletion_ticks)
        .unwrap_or_else(|| panic!("survival probe provisioning wait overflowed"));

    let offered_food_hydration = offerings
        .iter()
        .try_fold(0_u128, |total, (food, mass)| {
            let contribution = u128::from(mass.milligrams())
                .checked_mul(u128::from(food.hydration_microliters_per_milligram()))?;
            total.checked_add(contribution)
        })
        .unwrap_or_else(|| panic!("survival probe food hydration overflowed"));
    let hydration_deficit = hydration_loss
        .checked_mul(u128::from(provisioning_wait_ticks))
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
    initialize_player_survival(registries, &mut state)
        .unwrap_or_else(|error| panic!("survival probe player initialization failed: {error}"));
    let ambient_meal = seed_stockpile(&mut state, meal_mass, StockpileStorageProfile::solid_only());
    let witness_mass = offerings[witness_index].1;
    let preserved_reserve = seed_stockpile(&mut state, witness_mass, preserved_profile);
    let prepared = offerings
        .iter()
        .map(|(food, mass)| {
            let lot = seed_lot(
                registries,
                &mut state,
                ambient_meal,
                food.commodity(),
                *mass,
                ROOM_TEMPERATURE,
            );
            (*food, *mass, lot)
        })
        .collect::<Vec<_>>();
    let ambient_witness = prepared[witness_index].2;
    let preserved_witness = seed_lot(
        registries,
        &mut state,
        preserved_reserve,
        witness_food.commodity(),
        witness_mass,
        ROOM_TEMPERATURE,
    );
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
    let preservation_age_saved = ambient_age - preserved_age;

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
    let energy_deficit = physiology
        .maximum_metabolic_energy()
        .checked_sub(before.metabolic_energy())
        .unwrap_or_else(|| panic!("survival probe metabolic reserve exceeded authored maximum"));
    let hydration_deficit = physiology
        .maximum_hydration()
        .checked_sub(before.hydration())
        .unwrap_or_else(|| panic!("survival probe hydration reserve exceeded authored maximum"));
    let energy_deficit_ppm = u32::try_from(
        energy_deficit
            .nanojoules()
            .checked_mul(1_000_000)
            .map(|scaled| scaled / physiology.maximum_metabolic_energy().nanojoules())
            .unwrap_or_else(|| panic!("survival probe energy deficit normalization overflowed")),
    )
    .unwrap_or_else(|_| panic!("survival probe energy deficit normalization exceeded u32"));
    let hydration_deficit_ppm = u32::try_from(
        u128::from(hydration_deficit.microliters())
            .checked_mul(1_000_000)
            .map(|scaled| scaled / u128::from(physiology.maximum_hydration().microliters()))
            .unwrap_or_else(|| panic!("survival probe hydration deficit normalization overflowed")),
    )
    .unwrap_or_else(|_| panic!("survival probe hydration deficit normalization exceeded u32"));
    let energy_pressure = energy_deficit
        .nanojoules()
        .checked_mul(u128::from(physiology.maximum_hydration().microliters()))
        .unwrap_or_else(|| panic!("survival probe normalized energy pressure overflowed"));
    let hydration_pressure = u128::from(hydration_deficit.microliters())
        .checked_mul(physiology.maximum_metabolic_energy().nanojoules())
        .unwrap_or_else(|| panic!("survival probe normalized hydration pressure overflowed"));
    let provisioning_priority = match hydration_pressure.cmp(&energy_pressure) {
        std::cmp::Ordering::Greater => ProvisioningPriority::Hydration,
        std::cmp::Ordering::Less => ProvisioningPriority::MetabolicEnergy,
        std::cmp::Ordering::Equal => ProvisioningPriority::Balanced,
    };
    let drink_first = match provisioning_priority {
        ProvisioningPriority::Hydration => true,
        ProvisioningPriority::MetabolicEnergy => false,
        ProvisioningPriority::Balanced => mix64(seed ^ 0x5052_4F56_4953_494F).is_multiple_of(2),
    };
    let diet_quality_before = before.diet_quality_ppm();
    let (meal, drank, action_order) = if drink_first {
        let drank = validate_drink(registries, &state, drink_store, drink_volume)
            .unwrap_or_else(|error| panic!("survival probe drinking validation failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("survival probe drinking commit failed: {error}"));
        let meal = validate_eat(registries, &state, ambient_meal, &selections)
            .unwrap_or_else(|error| panic!("survival probe varied meal validation failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("survival probe varied meal commit failed: {error}"));
        (meal, drank, "drink->eat")
    } else {
        let meal = validate_eat(registries, &state, ambient_meal, &selections)
            .unwrap_or_else(|error| panic!("survival probe varied meal validation failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("survival probe varied meal commit failed: {error}"));
        let drank = validate_drink(registries, &state, drink_store, drink_volume)
            .unwrap_or_else(|error| panic!("survival probe drinking validation failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("survival probe drinking commit failed: {error}"));
        (meal, drank, "eat->drink")
    };
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

    assert!(!drank.hydration_gained().is_zero());
    assert_eq!(
        state
            .inventory()
            .get_lot(preserved_witness)
            .map(|lot| lot.mass()),
        Some(witness_mass),
        "food rotation must retain the fresher preserved witness instead of consuming it first"
    );

    let after = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("survival probe player disappeared after provisioning"));
    assert!(after.metabolic_energy() > before.metabolic_energy());
    assert!(after.hydration() > before.hydration());
    let reserve_recovered = after.metabolic_energy() > before.metabolic_energy()
        && after.hydration() > before.hydration();
    let diet_quality_gain_ppm = after.diet_quality_ppm().saturating_sub(diet_quality_before);
    if selected_categories.len() == authored_categories.len() {
        assert!(
            diet_quality_gain_ppm > 0,
            "a meal covering every authored dietary category must improve limiting diet quality"
        );
    } else {
        assert_eq!(
            diet_quality_gain_ppm, 0,
            "a meal that leaves one equally depleted dietary category untouched must not improve limiting diet quality"
        );
    }
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
        "PLAYABLE SURVIVAL seed=0x{seed:016X} catalog=registry-derived world-bootstrap=[authored-food,authored-drink,storage-profile] player-present-from=t0 food-rotation=[witness:{} elapsed:{age_ticks}t preservation:{preservation_multiplier_ppm}ppm ambient-age:{ambient_age}t preserved-age:{preserved_age}t age-saved:{preservation_age_saved}t consume:older-ambient retain-preserved:{}mg] depletion={depletion_ticks}t wait={provisioning_wait_ticks}t provisioning=[priority:{} action-order:{action_order}] meal=[foods:{food_label} categories:{category_label} mass:{}mg energy:+{}nJ nutrition:+{}ppm diet-quality:{}->{}ppm] drink=[fluid:{} volume:{}uL hydration:+{}uL] matter=conserved fluid=conserved tick={}",
        witness_food.commodity().value(),
        witness_mass.milligrams(),
        provisioning_priority.label(),
        meal.total_mass().milligrams(),
        meal.energy_gained().nanojoules(),
        meal.nutrition_gained().total_ppm(),
        diet_quality_before,
        after.diet_quality_ppm(),
        drink_fluid.value(),
        drank.volume().microliters(),
        drank.hydration_gained().microliters(),
        state.tick().value(),
    );
    let review = SurvivalProvisioningReview {
        preservation_age_saved_ticks: preservation_age_saved,
        selected_category_count: selected_categories.len(),
        authored_category_count: authored_categories.len(),
        provisioning_priority,
        energy_deficit_ppm,
        hydration_deficit_ppm,
        diet_quality_gain_ppm,
        retained_preserved_mass_mg: witness_mass.milligrams(),
        reserve_recovered,
    };
    std::println!(
        "SURVIVAL REVIEW fantasy=prepare+provision evidence=[food-rotation:{} preservation-age-saved:{}t diet=[selected:{}/{} limiting-quality-gain:{}ppm] pressure=[energy:{}ppm hydration:{}ppm dominant:{}] retained-preserved:{}mg reserve-recovered:{}]",
        review.preservation_age_saved_ticks > 0 && review.retained_preserved_mass_mg > 0,
        review.preservation_age_saved_ticks,
        review.selected_category_count,
        review.authored_category_count,
        review.diet_quality_gain_ppm,
        review.energy_deficit_ppm,
        review.hydration_deficit_ppm,
        review.provisioning_priority.label(),
        review.retained_preserved_mass_mg,
        review.reserve_recovered,
    );
    review
}

pub(super) fn run_survival_provisioning_probe(registries: &Registries, seed: u64) {
    let _ = evaluate_survival_provisioning_probe(registries, seed);
}
