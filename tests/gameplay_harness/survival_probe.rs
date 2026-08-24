//! Bounded survival-provisioning gameplay probe over authored food, preservation, and finite drink.

use std::collections::{BTreeMap, BTreeSet};

use deep_hearth::content::gameplay_fixture::{
    seed_fluid_store, seed_lot, seed_player_survival_at_hunger_warning,
    seed_player_survival_at_hydration_warning, seed_stockpile,
};
use deep_hearth::content::{
    ENERGY_STONE_FLYWHEEL_DRIVE, EQUIPMENT_STONE_HAND_CRANK, MANUAL_POWER_HAND_CRANK,
    MATERIAL_COPPER, PROSPECTING_FIELD_INSPECTION,
};
use deep_hearth::core::quantity::{Energy, Mass, Temperature, Volume};
use deep_hearth::core::state::{AppState, validate_loaded_state};
use deep_hearth::core::time::WorldSeed;
use deep_hearth::energy::validate_assemble_energy_store;
use deep_hearth::equipment::validate_assemble_equipment;
use deep_hearth::fluid::calculate_fluid_volume_accounting;
use deep_hearth::geology::{FieldProspectingRequest, validate_start_field_prospecting};
use deep_hearth::inventory::{MaterialLotId, MaterialLotSelection, StockpileStorageProfile};
use deep_hearth::labor::{ManualPowerRequest, validate_start_manual_power};
use deep_hearth::matter::calculate_matter_accounting;
use deep_hearth::registry::Registries;
use deep_hearth::simulation::advance_tick;
use deep_hearth::spatial::{VoxelBounds, VoxelCoord};
use deep_hearth::survival::{
    DrinkDefinition, DrinkError, EatError, FoodCategory, FoodDefinition, FoodFreshness,
    assess_food_freshness, assess_survival, initialize_player_survival, validate_drink,
    validate_eat,
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
enum ProvisioningPriority {
    MetabolicEnergy,
    Hydration,
    Balanced,
}

impl ProvisioningPriority {
    const fn label(self) -> &'static str {
        match self {
            Self::MetabolicEnergy => "energy",
            Self::Hydration => "hydration",
            Self::Balanced => "balanced",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DietProvisioningPolicy {
    CompactCalories,
    BalancedRecovery,
}

impl DietProvisioningPolicy {
    const fn label(self) -> &'static str {
        match self {
            Self::CompactCalories => "compact-calories",
            Self::BalancedRecovery => "balanced-recovery",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SurvivalCaseReview {
    policy: DietProvisioningPolicy,
    meal_mass_mg: u64,
    drink_volume_ul: u64,
    selected_category_count: usize,
    diet_quality_before_ppm: u32,
    diet_quality_after_ppm: u32,
    recovery_rate_before_ppm_per_tick: u32,
    recovery_rate_after_ppm_per_tick: u32,
    reserve_recovered: bool,
    preservation_age_saved_ticks: u64,
    retained_preserved_mass_mg: u64,
    energy_deficit_ppm: u32,
    hydration_deficit_ppm: u32,
    provisioning_priority: ProvisioningPriority,
}

fn selected_food_indices(foods: &[FoodDefinition], policy: DietProvisioningPolicy) -> Vec<usize> {
    let mut indices = (0..foods.len()).collect::<Vec<_>>();
    match policy {
        DietProvisioningPolicy::BalancedRecovery => indices,
        DietProvisioningPolicy::CompactCalories => {
            indices.sort_by(|left, right| {
                foods[*right]
                    .dietary_energy()
                    .nanojoules_per_milligram()
                    .cmp(&foods[*left].dietary_energy().nanojoules_per_milligram())
                    .then_with(|| foods[*left].category().cmp(&foods[*right].category()))
            });
            indices.truncate(indices.len().min(2));
            indices
        }
    }
}

fn normalized_energy_deficit_ppm(
    maximum: deep_hearth::core::quantity::Energy,
    current: deep_hearth::core::quantity::Energy,
) -> u32 {
    let deficit = maximum
        .checked_sub(current)
        .unwrap_or_else(|| panic!("survival probe metabolic reserve exceeded authored maximum"));
    u32::try_from(
        deficit
            .nanojoules()
            .checked_mul(1_000_000)
            .map(|scaled| scaled / maximum.nanojoules())
            .unwrap_or_else(|| panic!("survival probe energy deficit normalization overflowed")),
    )
    .unwrap_or_else(|_| panic!("survival probe energy deficit normalization exceeded u32"))
}

fn normalized_hydration_deficit_ppm(maximum: Volume, current: Volume) -> u32 {
    let deficit = maximum
        .checked_sub(current)
        .unwrap_or_else(|| panic!("survival probe hydration reserve exceeded authored maximum"));
    u32::try_from(
        u128::from(deficit.microliters())
            .checked_mul(1_000_000)
            .map(|scaled| scaled / u128::from(maximum.microliters()))
            .unwrap_or_else(|| panic!("survival probe hydration deficit normalization overflowed")),
    )
    .unwrap_or_else(|_| panic!("survival probe hydration deficit normalization exceeded u32"))
}

fn provisioning_priority_from_reserves(
    maximum_energy: Energy,
    current_energy: Energy,
    maximum_hydration: Volume,
    current_hydration: Volume,
) -> ProvisioningPriority {
    let energy_deficit = maximum_energy
        .checked_sub(current_energy)
        .unwrap_or_else(|| panic!("survival probe metabolic reserve exceeded authored maximum"));
    let hydration_deficit = maximum_hydration
        .checked_sub(current_hydration)
        .unwrap_or_else(|| panic!("survival probe hydration reserve exceeded authored maximum"));
    let energy_pressure = energy_deficit
        .nanojoules()
        .checked_mul(u128::from(maximum_hydration.microliters()))
        .unwrap_or_else(|| panic!("survival probe normalized energy pressure overflowed"));
    let hydration_pressure = u128::from(hydration_deficit.microliters())
        .checked_mul(maximum_energy.nanojoules())
        .unwrap_or_else(|| panic!("survival probe normalized hydration pressure overflowed"));
    match hydration_pressure.cmp(&energy_pressure) {
        std::cmp::Ordering::Greater => ProvisioningPriority::Hydration,
        std::cmp::Ordering::Less => ProvisioningPriority::MetabolicEnergy,
        std::cmp::Ordering::Equal => ProvisioningPriority::Balanced,
    }
}

struct ProvisioningWorld<'a> {
    foods: &'a [FoodDefinition],
    offered_masses: &'a [Mass],
    witness_index: usize,
    preserved_profile: StockpileStorageProfile,
    preservation_multiplier_ppm: u32,
    age_ticks: u64,
    provisioning_wait_ticks: u64,
    target_absorbed_energy: u128,
    drink: DrinkDefinition,
}

fn run_provisioning_case(
    registries: &Registries,
    seed: u64,
    world: &ProvisioningWorld<'_>,
    policy: DietProvisioningPolicy,
) -> SurvivalCaseReview {
    let ProvisioningWorld {
        foods,
        offered_masses,
        witness_index,
        preserved_profile,
        preservation_multiplier_ppm,
        age_ticks,
        provisioning_wait_ticks,
        target_absorbed_energy,
        drink,
    } = *world;
    let physiology = registries.survival().physiology();
    let selected_indices = selected_food_indices(foods, policy);
    assert!(!selected_indices.is_empty());
    let category_target = target_absorbed_energy
        .div_ceil(selected_indices.len() as u128)
        .max(1);
    let selected_masses = selected_indices
        .iter()
        .map(|index| mass_for_target_energy(foods[*index], category_target))
        .collect::<Vec<_>>();
    for (index, selected_mass) in selected_indices.iter().zip(&selected_masses) {
        assert!(
            *selected_mass <= offered_masses[*index],
            "survival probe offered food must cover every matched-policy portion"
        );
    }
    let offered_food_hydration = selected_indices
        .iter()
        .zip(&selected_masses)
        .try_fold(0_u128, |total, (index, mass)| {
            let contribution = u128::from(mass.milligrams()).checked_mul(u128::from(
                foods[*index].hydration_microliters_per_milligram(),
            ))?;
            total.checked_add(contribution)
        })
        .unwrap_or_else(|| panic!("survival probe food hydration overflowed"));
    let hydration_deficit = u128::from(physiology.hydration_loss_per_tick().microliters())
        .checked_mul(u128::from(provisioning_wait_ticks))
        .unwrap_or_else(|| panic!("survival probe hydration deficit overflowed"));
    let target_drink_gain = hydration_deficit
        .saturating_sub(offered_food_hydration)
        .max(1);
    let drink_volume = target_drink_gain
        .checked_mul(1_000_000)
        .unwrap_or_else(|| panic!("survival probe drink scaling overflowed"))
        .div_ceil(u128::from(drink.hydration_multiplier_ppm()))
        .max(1);
    let drink_volume = Volume::from_microliters(
        u64::try_from(drink_volume)
            .unwrap_or_else(|_| panic!("survival probe drink volume exceeds authoritative range")),
    );
    let drink_capacity = drink_volume
        .checked_add(drink_volume)
        .unwrap_or_else(|| panic!("survival probe drink capacity overflowed"));
    let ambient_capacity = offered_masses
        .iter()
        .try_fold(Mass::ZERO, |total, mass| total.checked_add(*mass))
        .unwrap_or_else(|| panic!("survival probe offered-food capacity overflowed"));
    let witness_food = foods[witness_index];
    let witness_mass = offered_masses[witness_index];

    let mut state = AppState::new(WorldSeed::new(seed));
    initialize_player_survival(registries, &mut state)
        .unwrap_or_else(|error| panic!("survival probe player initialization failed: {error}"));
    let ambient_meal = seed_stockpile(
        &mut state,
        ambient_capacity,
        StockpileStorageProfile::solid_only(),
    );
    let preserved_reserve = seed_stockpile(&mut state, witness_mass, preserved_profile);
    let prepared = foods
        .iter()
        .zip(offered_masses)
        .map(|(food, mass)| {
            let lot = seed_lot(
                registries,
                &mut state,
                ambient_meal,
                food.commodity(),
                *mass,
                ROOM_TEMPERATURE,
            );
            (*food, lot)
        })
        .collect::<Vec<_>>();
    let ambient_witness = prepared[witness_index].1;
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
        drink.fluid(),
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
    let preservation_age_saved_ticks = ambient_age - preserved_age;
    advance_exact(registries, &mut state, provisioning_wait_ticks - age_ticks);

    let before = assess_survival(registries, &state)
        .unwrap_or_else(|| panic!("survival probe player disappeared before provisioning"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("survival probe initial matter audit failed: {error}"));
    let fluid_before = calculate_fluid_volume_accounting(&state)
        .unwrap_or_else(|error| panic!("survival probe initial fluid audit failed: {error}"));
    let energy_deficit_ppm = normalized_energy_deficit_ppm(
        physiology.maximum_metabolic_energy(),
        before.metabolic_energy(),
    );
    let hydration_deficit_ppm =
        normalized_hydration_deficit_ppm(physiology.maximum_hydration(), before.hydration());
    let provisioning_priority = provisioning_priority_from_reserves(
        physiology.maximum_metabolic_energy(),
        before.metabolic_energy(),
        physiology.maximum_hydration(),
        before.hydration(),
    );
    let drink_first = match provisioning_priority {
        ProvisioningPriority::Hydration => true,
        ProvisioningPriority::MetabolicEnergy => false,
        ProvisioningPriority::Balanced => mix64(seed ^ 0x5052_4F56_4953_494F).is_multiple_of(2),
    };
    let selections = selected_indices
        .iter()
        .zip(&selected_masses)
        .map(|(index, mass)| MaterialLotSelection::new(prepared[*index].1, *mass))
        .collect::<Vec<_>>();
    let selected_categories = selected_indices
        .iter()
        .map(|index| foods[*index].category())
        .collect::<Vec<_>>();
    assert_eq!(
        selected_categories
            .iter()
            .copied()
            .collect::<BTreeSet<_>>()
            .len(),
        selected_categories.len(),
        "survival probe meal selection must not duplicate a dietary category"
    );
    let diet_quality_before = before.diet_quality_ppm();
    let recovery_rate_before = before.diet_supported_vitality_recovery_ppm_per_tick();
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
    let reserve_recovered = after.metabolic_energy() > before.metabolic_energy()
        && after.hydration() > before.hydration();
    assert_eq!(
        after.metabolic_energy(),
        physiology.maximum_metabolic_energy()
    );
    assert_eq!(after.hydration(), physiology.maximum_hydration());
    let recovery_rate_after = after.diet_supported_vitality_recovery_ppm_per_tick();
    if selected_categories.len() == foods.len() {
        assert!(after.diet_quality_ppm() > diet_quality_before);
        assert!(recovery_rate_after > recovery_rate_before);
    } else {
        assert_eq!(after.diet_quality_ppm(), diet_quality_before);
        assert_eq!(recovery_rate_after, recovery_rate_before);
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

    if std::env::var_os("DEEP_HEARTH_GAMEPLAY_VERBOSE").is_some() {
        let available_categories = foods
            .iter()
            .map(|food| format!("{:?}", food.category()))
            .collect::<Vec<_>>()
            .join("+");
        let selected_categories = selected_categories
            .iter()
            .map(|category| format!("{category:?}"))
            .collect::<Vec<_>>()
            .join("+");
        std::println!(
            "PLAYABLE SURVIVAL seed=0x{seed:016X} mode=matched-policy policy={} catalog=registry-derived world-bootstrap=[authored-food,authored-drink,storage-profile] player-present-from=t0 available-categories={available_categories} selected-categories={selected_categories} food-rotation=[witness:{} elapsed:{age_ticks}t preservation:{preservation_multiplier_ppm}ppm ambient-age:{ambient_age}t preserved-age:{preserved_age}t age-saved:{preservation_age_saved_ticks}t consume:older-ambient retain-preserved:{}mg] wait={provisioning_wait_ticks}t provisioning=[priority:{} action-order:{action_order}] meal=[mass:{}mg energy:+{}nJ nutrition:+{}ppm diet-quality:{}->{}ppm recovery-rate:{}->{}ppm/t] drink=[fluid:{} volume:{}uL hydration:+{}uL] reserves=restored matter=conserved fluid=conserved tick={}",
            policy.label(),
            witness_food.commodity().value(),
            witness_mass.milligrams(),
            provisioning_priority.label(),
            meal.total_mass().milligrams(),
            meal.energy_gained().nanojoules(),
            meal.nutrition_gained().total_ppm(),
            diet_quality_before,
            after.diet_quality_ppm(),
            recovery_rate_before,
            recovery_rate_after,
            drink.fluid().value(),
            drank.volume().microliters(),
            drank.hydration_gained().microliters(),
            state.tick().value(),
        );
    }

    SurvivalCaseReview {
        policy,
        meal_mass_mg: meal.total_mass().milligrams(),
        drink_volume_ul: drank.volume().microliters(),
        selected_category_count: selected_categories.len(),
        diet_quality_before_ppm: diet_quality_before,
        diet_quality_after_ppm: after.diet_quality_ppm(),
        recovery_rate_before_ppm_per_tick: recovery_rate_before,
        recovery_rate_after_ppm_per_tick: recovery_rate_after,
        reserve_recovered,
        preservation_age_saved_ticks,
        retained_preserved_mass_mg: witness_mass.milligrams(),
        energy_deficit_ppm,
        hydration_deficit_ppm,
        provisioning_priority,
    }
}

fn evaluate_survival_pressure_response_probe(registries: &Registries, seed: u64) {
    let dry_foods = registries
        .survival()
        .foods()
        .copied()
        .filter(|food| food.hydration_microliters_per_milligram() == 0)
        .collect::<Vec<_>>();
    assert!(
        !dry_foods.is_empty(),
        "survival pressure probe requires one authored dry food so hunger and thirst actions remain physically distinct"
    );
    let dry_food =
        dry_foods[usize::try_from(mix64(seed ^ 0x5052_4553_5355_5245) % dry_foods.len() as u64)
            .unwrap_or_else(|_| unreachable!("dry-food index fits usize"))];
    let drinks = registries.survival().drinks().copied().collect::<Vec<_>>();
    assert!(
        !drinks.is_empty(),
        "survival pressure probe requires one authored drink"
    );
    let drink = drinks[usize::try_from(mix64(seed ^ 0x5052_4553_4452_494E) % drinks.len() as u64)
        .unwrap_or_else(|_| unreachable!("pressure-probe drink index fits usize"))];
    let food_mass = Mass::from_milligrams(1);
    let drink_volume = Volume::from_microliters(1);
    let physiology = registries.survival().physiology();

    let mut hunger = AppState::new(WorldSeed::new(seed ^ 0x4855_4E47_4552_0001));
    seed_player_survival_at_hunger_warning(registries, &mut hunger);
    let hunger_food_store = seed_stockpile(
        &mut hunger,
        food_mass,
        StockpileStorageProfile::solid_only(),
    );
    let hunger_food = seed_lot(
        registries,
        &mut hunger,
        hunger_food_store,
        dry_food.commodity(),
        food_mass,
        ROOM_TEMPERATURE,
    );
    let hunger_drink_store = seed_fluid_store(
        registries,
        &mut hunger,
        drink_volume,
        drink.fluid(),
        drink_volume,
        ROOM_TEMPERATURE,
    );
    let hunger_before = assess_survival(registries, &hunger)
        .unwrap_or_else(|| panic!("hunger-pressure player disappeared"));
    let hunger_priority = provisioning_priority_from_reserves(
        physiology.maximum_metabolic_energy(),
        hunger_before.metabolic_energy(),
        physiology.maximum_hydration(),
        hunger_before.hydration(),
    );
    assert_eq!(hunger_priority, ProvisioningPriority::MetabolicEnergy);
    assert_eq!(
        validate_drink(registries, &hunger, hunger_drink_store, drink_volume).err(),
        Some(DrinkError::NoHydrationGain {
            volume: drink_volume,
        })
    );
    let hunger_meal = validate_eat(
        registries,
        &hunger,
        hunger_food_store,
        &[MaterialLotSelection::new(hunger_food, food_mass)],
    )
    .unwrap_or_else(|error| panic!("hunger-pressure dry food should be useful: {error}"))
    .commit(&mut hunger)
    .unwrap_or_else(|error| panic!("hunger-pressure meal commit failed: {error}"));
    assert!(!hunger_meal.energy_gained().is_zero());
    let hunger_after = assess_survival(registries, &hunger)
        .unwrap_or_else(|| panic!("hunger-pressure player disappeared after eating"));
    assert!(hunger_after.metabolic_energy() > hunger_before.metabolic_energy());

    let mut thirst = AppState::new(WorldSeed::new(seed ^ 0x5448_4952_5354_0002));
    seed_player_survival_at_hydration_warning(registries, &mut thirst);
    let thirst_food_store = seed_stockpile(
        &mut thirst,
        food_mass,
        StockpileStorageProfile::solid_only(),
    );
    let thirst_food = seed_lot(
        registries,
        &mut thirst,
        thirst_food_store,
        dry_food.commodity(),
        food_mass,
        ROOM_TEMPERATURE,
    );
    let thirst_drink_store = seed_fluid_store(
        registries,
        &mut thirst,
        drink_volume,
        drink.fluid(),
        drink_volume,
        ROOM_TEMPERATURE,
    );
    let thirst_before = assess_survival(registries, &thirst)
        .unwrap_or_else(|| panic!("thirst-pressure player disappeared"));
    let thirst_priority = provisioning_priority_from_reserves(
        physiology.maximum_metabolic_energy(),
        thirst_before.metabolic_energy(),
        physiology.maximum_hydration(),
        thirst_before.hydration(),
    );
    assert_eq!(thirst_priority, ProvisioningPriority::Hydration);
    assert_eq!(
        validate_eat(
            registries,
            &thirst,
            thirst_food_store,
            &[MaterialLotSelection::new(thirst_food, food_mass)],
        )
        .err(),
        Some(EatError::NoReserveGain { mass: food_mass }),
        "dry food at full metabolic and nutrition reserves must be rejected specifically because it cannot improve any reserve"
    );
    let thirst_drink = validate_drink(registries, &thirst, thirst_drink_store, drink_volume)
        .unwrap_or_else(|error| panic!("thirst-pressure drink should be useful: {error}"))
        .commit(&mut thirst)
        .unwrap_or_else(|error| panic!("thirst-pressure drink commit failed: {error}"));
    assert!(!thirst_drink.hydration_gained().is_zero());
    let thirst_after = assess_survival(registries, &thirst)
        .unwrap_or_else(|| panic!("thirst-pressure player disappeared after drinking"));
    assert!(thirst_after.hydration() > thirst_before.hydration());
    validate_loaded_state(registries, &hunger)
        .unwrap_or_else(|error| panic!("hunger-pressure state audit failed: {error}"));
    validate_loaded_state(registries, &thirst)
        .unwrap_or_else(|error| panic!("thirst-pressure state audit failed: {error}"));
    if std::env::var_os("DEEP_HEARTH_GAMEPLAY_VERBOSE").is_some() {
        std::println!(
            "SURVIVAL PRESSURE seed=0x{seed:016X} matched-warning-worlds=[hunger:[priority:{} eat:useful drink:blocked-full-hydration] thirst:[priority:{} drink:useful dry-food:blocked-no-benefit]] response=pressure-sensitive canonical-actions=true",
            hunger_priority.label(),
            thirst_priority.label(),
        );
    }
}

fn evaluate_survival_work_pressure_probe(registries: &Registries, seed: u64) {
    let physiology = registries.survival().physiology();

    let mut prospecting = AppState::new(WorldSeed::new(seed ^ 0x5052_4F53_5045_4354));
    initialize_player_survival(registries, &mut prospecting)
        .unwrap_or_else(|error| panic!("work-pressure prospecting survival setup failed: {error}"));
    let region = VoxelBounds::new(VoxelCoord::new(24, -1, 0), VoxelCoord::new(25, 0, 1))
        .unwrap_or_else(|error| panic!("work-pressure prospecting bounds failed: {error}"));
    let prospecting_before = assess_survival(registries, &prospecting)
        .unwrap_or_else(|| panic!("work-pressure prospecting player disappeared"));
    let prospecting_start = validate_start_field_prospecting(
        registries,
        &prospecting,
        FieldProspectingRequest::new(PROSPECTING_FIELD_INSPECTION, region, MATERIAL_COPPER),
    )
    .unwrap_or_else(|error| panic!("work-pressure prospecting start failed: {error}"));
    let prospecting_work = prospecting_start.work();
    let prospecting_ticks = prospecting_work
        .completes_at()
        .value()
        .checked_sub(prospecting_work.started_at().value())
        .unwrap_or_else(|| unreachable!("validated prospecting completes after it starts"));
    prospecting_start
        .commit(&mut prospecting)
        .unwrap_or_else(|error| panic!("work-pressure prospecting commit failed: {error}"));
    advance_exact(registries, &mut prospecting, prospecting_ticks);
    assert_eq!(
        prospecting.geological_knowledge().observations().count(),
        1,
        "matched work-pressure prospecting must persist its actual field observation"
    );
    let prospecting_after = assess_survival(registries, &prospecting)
        .unwrap_or_else(|| panic!("work-pressure prospecting player disappeared after work"));
    let prospecting_energy_deficit_ppm = normalized_energy_deficit_ppm(
        physiology.maximum_metabolic_energy(),
        prospecting_after.metabolic_energy(),
    );
    let prospecting_hydration_deficit_ppm = normalized_hydration_deficit_ppm(
        physiology.maximum_hydration(),
        prospecting_after.hydration(),
    );
    assert_eq!(
        prospecting_before.metabolic_energy(),
        physiology.maximum_metabolic_energy()
    );
    assert_eq!(
        prospecting_before.hydration(),
        physiology.maximum_hydration()
    );
    assert!(
        prospecting_hydration_deficit_ppm > prospecting_energy_deficit_ppm,
        "field prospecting should remain hydration-biased so observation work and strenuous power work create different survival pressures"
    );

    let mut power = AppState::new(WorldSeed::new(seed ^ 0x504F_5745_5257_4F52));
    let crank_profile = registries
        .equipment()
        .get_equipment(EQUIPMENT_STONE_HAND_CRANK)
        .and_then(|definition| definition.assembly_profile())
        .unwrap_or_else(|| panic!("work-pressure stone crank lost its assembly route"));
    let drive_profile = registries
        .energy()
        .get_store(ENERGY_STONE_FLYWHEEL_DRIVE)
        .and_then(|definition| definition.assembly_profile())
        .unwrap_or_else(|| panic!("work-pressure stone flywheel lost its assembly route"));
    let component_capacity = crank_profile
        .inputs()
        .iter()
        .chain(drive_profile.inputs())
        .try_fold(Mass::ZERO, |total, input| total.checked_add(input.mass()))
        .unwrap_or_else(|| panic!("work-pressure primitive power component mass overflowed"));
    let component_source = seed_stockpile(
        &mut power,
        component_capacity,
        StockpileStorageProfile::solid_only(),
    );
    for input in crank_profile.inputs().iter().chain(drive_profile.inputs()) {
        seed_lot(
            registries,
            &mut power,
            component_source,
            input.commodity(),
            input.mass(),
            ROOM_TEMPERATURE,
        );
    }
    let crank = validate_assemble_equipment(
        registries,
        &power,
        EQUIPMENT_STONE_HAND_CRANK,
        component_source,
    )
    .unwrap_or_else(|error| panic!("work-pressure stone crank assembly failed: {error}"))
    .commit(&mut power)
    .unwrap_or_else(|error| panic!("work-pressure stone crank assembly commit failed: {error}"));
    let drive = validate_assemble_energy_store(
        registries,
        &power,
        ENERGY_STONE_FLYWHEEL_DRIVE,
        component_source,
    )
    .unwrap_or_else(|error| panic!("work-pressure stone flywheel assembly failed: {error}"))
    .commit(&mut power)
    .unwrap_or_else(|error| panic!("work-pressure stone flywheel assembly commit failed: {error}"));
    initialize_player_survival(registries, &mut power).unwrap_or_else(|error| {
        panic!("work-pressure manual-power survival setup failed: {error}")
    });
    let requested_energy = registries
        .energy()
        .get_store(ENERGY_STONE_FLYWHEEL_DRIVE)
        .map(|definition| definition.capacity())
        .unwrap_or_else(|| panic!("work-pressure stone flywheel definition disappeared"));
    let power_before = assess_survival(registries, &power)
        .unwrap_or_else(|| panic!("work-pressure manual-power player disappeared"));
    let power_start = validate_start_manual_power(
        registries,
        &power,
        ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, crank, drive, requested_energy),
    )
    .unwrap_or_else(|error| panic!("work-pressure manual-power start failed: {error}"));
    let power_work = power_start.work();
    let power_ticks = power_work
        .completes_at()
        .value()
        .checked_sub(power_work.started_at().value())
        .unwrap_or_else(|| unreachable!("validated manual power completes after it starts"));
    power_start
        .commit(&mut power)
        .unwrap_or_else(|error| panic!("work-pressure manual-power commit failed: {error}"));
    advance_exact(registries, &mut power, power_ticks);
    assert_eq!(
        power.energy().get_store(drive).map(|store| store.stored()),
        Some(requested_energy),
        "matched work-pressure manual labor must create the requested finite stored work"
    );
    let power_after = assess_survival(registries, &power)
        .unwrap_or_else(|| panic!("work-pressure manual-power player disappeared after work"));
    let power_energy_deficit_ppm = normalized_energy_deficit_ppm(
        physiology.maximum_metabolic_energy(),
        power_after.metabolic_energy(),
    );
    let power_hydration_deficit_ppm =
        normalized_hydration_deficit_ppm(physiology.maximum_hydration(), power_after.hydration());
    assert_eq!(
        power_before.metabolic_energy(),
        physiology.maximum_metabolic_energy()
    );
    assert_eq!(power_before.hydration(), physiology.maximum_hydration());
    assert!(
        power_energy_deficit_ppm > power_hydration_deficit_ppm,
        "sustained manual power should be calorie-biased so the player's chosen labor changes the dominant survival pressure"
    );

    validate_loaded_state(registries, &prospecting)
        .unwrap_or_else(|error| panic!("work-pressure prospecting state audit failed: {error}"));
    validate_loaded_state(registries, &power)
        .unwrap_or_else(|error| panic!("work-pressure manual-power state audit failed: {error}"));
    if std::env::var_os("DEEP_HEARTH_GAMEPLAY_VERBOSE").is_some() {
        std::println!(
            "SURVIVAL WORK PRESSURE seed=0x{seed:016X} matched-full-reserve-work=[prospecting:[{}t energy:{}ppm hydration:{}ppm dominant:hydration] manual-power:[{}t energy:{}ppm hydration:{}ppm dominant:energy stored-work:{}nJ]] activity-changes-dominant-pressure=true canonical-actions=true",
            prospecting_ticks,
            prospecting_energy_deficit_ppm,
            prospecting_hydration_deficit_ppm,
            power_ticks,
            power_energy_deficit_ppm,
            power_hydration_deficit_ppm,
            requested_energy.nanojoules(),
        );
    }
}

fn evaluate_survival_provisioning_probe(registries: &Registries, seed: u64) {
    evaluate_survival_pressure_response_probe(registries, seed);
    evaluate_survival_work_pressure_probe(registries, seed);
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
    let foods = foods_by_category
        .iter()
        .enumerate()
        .map(|(index, (category, options))| {
            let choice = usize::try_from(
                mix64(seed ^ category_salt(*category) ^ index as u64) % options.len() as u64,
            )
            .unwrap_or_else(|_| unreachable!("food option index fits usize"));
            options[choice]
        })
        .collect::<Vec<_>>();
    let compact_indices = selected_food_indices(&foods, DietProvisioningPolicy::CompactCalories);
    let balanced_indices = selected_food_indices(&foods, DietProvisioningPolicy::BalancedRecovery);
    let witness_slot =
        usize::try_from(mix64(seed ^ 0x5052_4553_5749_544E) % compact_indices.len() as u64)
            .unwrap_or_else(|_| unreachable!("preservation witness index fits usize"));
    let witness_index = compact_indices[witness_slot];
    let witness_food = foods[witness_index];
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
    let ticks_per_day = registries.core().calendar().ticks_per_day();
    let provisioning_base = ticks_per_day
        .checked_mul(2)
        .map(|ticks| ticks / 3)
        .unwrap_or_else(|| panic!("survival probe provisioning horizon overflowed"));
    let provisioning_jitter = (ticks_per_day / 12).max(1);
    let provisioning_wait_ticks = provisioning_base
        .checked_add(mix64(seed ^ 0x4441_5946_5241_4354) % provisioning_jitter)
        .unwrap_or_else(|| panic!("survival probe provisioning wait overflowed"));
    assert!(provisioning_wait_ticks > age_ticks);
    let target_absorbed_energy = physiology
        .basal_energy_cost_per_tick()
        .nanojoules()
        .checked_mul(u128::from(provisioning_wait_ticks))
        .unwrap_or_else(|| panic!("survival probe meal-energy target overflowed"));
    assert!(target_absorbed_energy < physiology.maximum_metabolic_energy().nanojoules());
    let compact_target = target_absorbed_energy
        .div_ceil(compact_indices.len() as u128)
        .max(1);
    let balanced_target = target_absorbed_energy
        .div_ceil(balanced_indices.len() as u128)
        .max(1);
    let offered_masses = foods
        .iter()
        .enumerate()
        .map(|(index, food)| {
            let balanced = mass_for_target_energy(*food, balanced_target);
            if compact_indices.contains(&index) {
                balanced.max(mass_for_target_energy(*food, compact_target))
            } else {
                balanced
            }
        })
        .collect::<Vec<_>>();
    let drinks = registries.survival().drinks().copied().collect::<Vec<_>>();
    assert!(
        !drinks.is_empty(),
        "survival gameplay is stale or unavailable: the runtime registry has no authored drinkable fluid"
    );
    let drink_index = usize::try_from(mix64(seed ^ 0x4452_494E_4B00_0001) % drinks.len() as u64)
        .unwrap_or_else(|_| unreachable!("drink index fits usize"));
    let drink = drinks[drink_index];
    let world = ProvisioningWorld {
        foods: &foods,
        offered_masses: &offered_masses,
        witness_index,
        preserved_profile,
        preservation_multiplier_ppm,
        age_ticks,
        provisioning_wait_ticks,
        target_absorbed_energy,
        drink,
    };

    let compact = run_provisioning_case(
        registries,
        seed,
        &world,
        DietProvisioningPolicy::CompactCalories,
    );
    let balanced = run_provisioning_case(
        registries,
        seed,
        &world,
        DietProvisioningPolicy::BalancedRecovery,
    );
    assert_eq!(compact.policy, DietProvisioningPolicy::CompactCalories);
    assert_eq!(balanced.policy, DietProvisioningPolicy::BalancedRecovery);
    assert_eq!(compact.energy_deficit_ppm, balanced.energy_deficit_ppm);
    assert_eq!(
        compact.hydration_deficit_ppm,
        balanced.hydration_deficit_ppm
    );
    assert_eq!(
        compact.provisioning_priority,
        balanced.provisioning_priority
    );
    assert_eq!(
        compact.preservation_age_saved_ticks,
        balanced.preservation_age_saved_ticks
    );
    assert_eq!(
        compact.retained_preserved_mass_mg,
        balanced.retained_preserved_mass_mg
    );
    assert!(compact.reserve_recovered && balanced.reserve_recovered);
    if foods.len() >= 3 {
        assert!(compact.selected_category_count < balanced.selected_category_count);
        assert!(compact.meal_mass_mg <= balanced.meal_mass_mg);
        assert!(balanced.diet_quality_after_ppm > compact.diet_quality_after_ppm);
        assert!(
            balanced.recovery_rate_after_ppm_per_tick > compact.recovery_rate_after_ppm_per_tick,
            "balanced provisioning must buy measurably stronger recovery resilience than the compact-calorie meal in the maintained survival horizon"
        );
    }
    let balanced_diet_quality_advantage_ppm = balanced
        .diet_quality_after_ppm
        .saturating_sub(compact.diet_quality_after_ppm);
    let recovery_rate_advantage_ppm_per_tick = balanced
        .recovery_rate_after_ppm_per_tick
        .saturating_sub(compact.recovery_rate_after_ppm_per_tick);
    let reserve_recovered = compact.reserve_recovered && balanced.reserve_recovered;
    std::println!(
        "SURVIVAL REVIEW seed=0x{seed:016X} fantasy=prepare+provision episode=[wait:{provisioning_wait_ticks}t available-categories:{}] matched-world-choice=[compact-calories:[selected:{} meal:{}mg drink:{}uL diet:{}->{}ppm recovery:{}->{}ppm/t] balanced:[selected:{} meal:{}mg drink:{}uL diet:{}->{}ppm recovery:{}->{}ppm/t]] tradeoff=[meal-mass-delta:+{}mg water-saved:+{}uL diet-quality-advantage:+{}ppm recovery-advantage:+{}ppm/t] pressure=[energy:{}ppm hydration:{}ppm dominant:{}] preservation=[age-saved:{}t retained:{}mg] reserve-recovered:{}",
        foods.len(),
        compact.selected_category_count,
        compact.meal_mass_mg,
        compact.drink_volume_ul,
        compact.diet_quality_before_ppm,
        compact.diet_quality_after_ppm,
        compact.recovery_rate_before_ppm_per_tick,
        compact.recovery_rate_after_ppm_per_tick,
        balanced.selected_category_count,
        balanced.meal_mass_mg,
        balanced.drink_volume_ul,
        balanced.diet_quality_before_ppm,
        balanced.diet_quality_after_ppm,
        balanced.recovery_rate_before_ppm_per_tick,
        balanced.recovery_rate_after_ppm_per_tick,
        balanced.meal_mass_mg.saturating_sub(compact.meal_mass_mg),
        compact
            .drink_volume_ul
            .saturating_sub(balanced.drink_volume_ul),
        balanced_diet_quality_advantage_ppm,
        recovery_rate_advantage_ppm_per_tick,
        compact.energy_deficit_ppm,
        compact.hydration_deficit_ppm,
        compact.provisioning_priority.label(),
        compact.preservation_age_saved_ticks,
        compact.retained_preserved_mass_mg,
        reserve_recovered,
    );
}

pub(super) fn run_survival_provisioning_probe(registries: &Registries, seed: u64) {
    evaluate_survival_provisioning_probe(registries, seed);
}
