//! Matched hunger/thirst warning-boundary response evidence.

use super::*;

pub(super) fn evaluate_survival_pressure_response_probe(registries: &Registries, seed: u64) {
    let mut dry_foods = registries
        .survival()
        .foods()
        .copied()
        .filter(|food| food.hydration_multiplier_ppm() == 0)
        .collect::<Vec<_>>();
    dry_foods.sort_by_key(|food| food.commodity());
    assert!(
        !dry_foods.is_empty(),
        "survival pressure probe requires one authored dry food so hunger and thirst actions remain physically distinct"
    );
    let dry_food =
        dry_foods[usize::try_from(mix64(seed ^ 0x5052_4553_5355_5245) % dry_foods.len() as u64)
            .unwrap_or_else(|_| unreachable!("dry-food index fits usize"))];
    let mut drinks = registries.survival().drinks().copied().collect::<Vec<_>>();
    drinks.sort_by_key(|drink| drink.fluid());
    assert!(
        !drinks.is_empty(),
        "survival pressure probe requires one authored drink"
    );
    let drink = drinks[usize::try_from(mix64(seed ^ 0x5052_4553_4452_494E) % drinks.len() as u64)
        .unwrap_or_else(|_| unreachable!("pressure-probe drink index fits usize"))];
    let food_mass = Mass::from_milligrams(1);
    let physiology = registries.survival().physiology();
    let drink_volume = physiology.direct_consumption().minimum_drink_volume();

    let mut hunger = AppState::new(WorldSeed::new(seed ^ 0x4855_4E47_4552_0001));
    let hunger_food_store = seed_stockpile(
        &mut hunger,
        food_mass,
        StockpileStorageProfile::unbounded_solid_only(),
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
    seed_player_survival_at_hunger_warning_boundary(registries, &mut hunger);
    let hunger_before = assess_survival(registries, &hunger)
        .unwrap_or_else(|| panic!("hunger-pressure player disappeared"));
    let hunger_priority = provisioning_priority_from_reserves(
        physiology.maximum_metabolic_energy(),
        hunger_before.metabolic_energy(),
        physiology.maximum_hydration(),
        hunger_before.hydration(),
    );
    assert_eq!(hunger_priority, ProvisioningPriority::MetabolicEnergy);
    let _ = validate_drink(registries, &hunger, hunger_drink_store, drink_volume)
        .unwrap_or_else(|error| {
            panic!(
                "timed drinking at full hydration should remain useful because basal loss creates capacity during the action: {error}"
            )
        });
    let mut hunger_baseline = hunger.clone();
    let hunger_meal = validate_eat(
        registries,
        &hunger,
        hunger_food_store,
        &[MaterialLotSelection::new(hunger_food, food_mass)],
    )
    .unwrap_or_else(|error| panic!("hunger-pressure dry food should be useful: {error}"))
    .commit(&mut hunger)
    .unwrap_or_else(|error| panic!("hunger-pressure meal commit failed: {error}"));
    assert!(!hunger_meal.energy_offered().is_zero());
    let hunger_ticks =
        finish_direct_consumption(registries, &mut hunger, hunger_meal.completes_at());
    advance_idle_ticks(
        registries,
        &mut hunger_baseline,
        hunger_ticks,
        "hunger-pressure no-meal baseline",
    );
    let hunger_after = assess_survival(registries, &hunger)
        .unwrap_or_else(|| panic!("hunger-pressure player disappeared after eating"));
    let hunger_baseline_after = assess_survival(registries, &hunger_baseline)
        .unwrap_or_else(|| panic!("hunger-pressure baseline player disappeared"));
    assert!(hunger_after.metabolic_energy() > hunger_baseline_after.metabolic_energy());

    let mut thirst = AppState::new(WorldSeed::new(seed ^ 0x5448_4952_5354_0002));
    let thirst_food_store = seed_stockpile(
        &mut thirst,
        food_mass,
        StockpileStorageProfile::unbounded_solid_only(),
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
    seed_player_survival_at_hydration_warning_boundary(registries, &mut thirst);
    let thirst_before = assess_survival(registries, &thirst)
        .unwrap_or_else(|| panic!("thirst-pressure player disappeared"));
    let thirst_priority = provisioning_priority_from_reserves(
        physiology.maximum_metabolic_energy(),
        thirst_before.metabolic_energy(),
        physiology.maximum_hydration(),
        thirst_before.hydration(),
    );
    assert_eq!(thirst_priority, ProvisioningPriority::Hydration);
    let _ = validate_eat(
        registries,
        &thirst,
        thirst_food_store,
        &[MaterialLotSelection::new(thirst_food, food_mass)],
    )
    .unwrap_or_else(|error| {
        panic!(
            "timed eating at full metabolic reserves should remain useful because basal cost creates capacity during the action: {error}"
        )
    });
    let mut thirst_baseline = thirst.clone();
    let thirst_drink = validate_drink(registries, &thirst, thirst_drink_store, drink_volume)
        .unwrap_or_else(|error| panic!("thirst-pressure drink should be useful: {error}"))
        .commit(&mut thirst)
        .unwrap_or_else(|error| panic!("thirst-pressure drink commit failed: {error}"));
    assert!(!thirst_drink.hydration_offered().is_zero());
    let thirst_ticks =
        finish_direct_consumption(registries, &mut thirst, thirst_drink.completes_at());
    advance_idle_ticks(
        registries,
        &mut thirst_baseline,
        thirst_ticks,
        "thirst-pressure no-drink baseline",
    );
    let thirst_after = assess_survival(registries, &thirst)
        .unwrap_or_else(|| panic!("thirst-pressure player disappeared after drinking"));
    let thirst_baseline_after = assess_survival(registries, &thirst_baseline)
        .unwrap_or_else(|| panic!("thirst-pressure baseline player disappeared"));
    assert!(thirst_after.hydration() > thirst_baseline_after.hydration());
    validate_loaded_state(registries, &hunger)
        .unwrap_or_else(|error| panic!("hunger-pressure state audit failed: {error}"));
    validate_loaded_state(registries, &thirst)
        .unwrap_or_else(|error| panic!("thirst-pressure state audit failed: {error}"));
    if std::env::var_os("DEEP_HEARTH_GAMEPLAY_VERBOSE").is_some() {
        reviewln!(
            "SURVIVAL PRESSURE seed=0x{seed:016X} matched-warning-boundary-worlds=[hunger:[priority:{} eat:targeted drink:legal-nontarget] thirst:[priority:{} drink:targeted dry-food:legal-nontarget]] response=pressure-sensitive counterfactual-benefit=true canonical-actions=true",
            hunger_priority.label(),
            thirst_priority.label(),
        );
    }
}
