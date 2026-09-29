//! Alignment contracts for survival meal policy planning.

use super::*;

#[test]
fn compact_meal_policy_matches_the_production_single_food_target_planner() {
    let registries = deep_hearth::content::build_registries();
    let mut state = AppState::new();
    seed_player_survival_at_hunger_warning_boundary(&registries, &mut state);
    let food = registries
        .survival()
        .foods()
        .next()
        .copied()
        .unwrap_or_else(|| panic!("survival registry lost all authored food"));
    let physiology = registries.survival().physiology();
    let current = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("survival planner fixture lost player"))
        .metabolic_energy();
    let expected = match deep_hearth::survival::project_minimum_meal_to_metabolic_target(
        physiology,
        food,
        current,
        physiology.maximum_metabolic_energy(),
    ) {
        Ok(Some(projection)) => projection.mass(),
        Ok(None) => physiology.direct_consumption().minimum_meal_mass(),
        Err(
            deep_hearth::survival::MealMetabolicProjectionError::TargetUnreachableWithinIntakeLimit {
                maximum_meal_mass,
            },
        ) => maximum_meal_mass,
        Err(error) => panic!("production meal projection failed: {error}"),
    };

    assert_eq!(
        desired_policy_meal_masses(&registries, &state, &[food], &[0]),
        vec![expected]
    );
}

#[test]
fn balanced_meal_policy_pays_authoritative_eating_time_metabolism_or_hits_intake_limit() {
    let registries = deep_hearth::content::build_registries();
    let mut state = AppState::new();
    seed_player_survival_at_hunger_warning_boundary(&registries, &mut state);
    let mut by_category = BTreeMap::new();
    for food in registries.survival().foods().copied() {
        by_category.entry(food.category()).or_insert(food);
    }
    let foods = by_category.values().copied().collect::<Vec<_>>();
    assert!(
        foods.len() >= 2,
        "balanced survival policy requires at least two authored food categories"
    );
    let selected = (0..foods.len()).collect::<Vec<_>>();
    let masses = desired_policy_meal_masses(&registries, &state, &foods, &selected);
    let physiology = registries.survival().physiology();
    let direct = physiology.direct_consumption();
    let total = masses
        .iter()
        .try_fold(Mass::ZERO, |sum, mass| sum.checked_add(*mass))
        .unwrap_or_else(|| panic!("balanced meal mass overflowed"));
    let duration = direct
        .meal_duration(total)
        .unwrap_or_else(|| panic!("balanced meal duration disappeared"));
    let meal_cost = deep_hearth::survival::project_survival_resource_budget(
        physiology,
        deep_hearth::survival::SurvivalExertion::REST,
        duration,
    )
    .unwrap_or_else(|error| panic!("balanced meal resource projection failed: {error:?}"))
    .metabolic_energy();
    let current = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("balanced meal fixture lost player"))
        .metabolic_energy();
    let reserve_gap = physiology
        .maximum_metabolic_energy()
        .checked_sub(current)
        .unwrap_or_else(|| unreachable!("fixture reserve is within authored maximum"));
    let required_offer = reserve_gap
        .checked_add(meal_cost)
        .unwrap_or_else(|| panic!("balanced meal required energy overflowed"));
    let offered = foods
        .iter()
        .zip(&masses)
        .try_fold(Energy::ZERO, |sum, (food, mass)| {
            sum.checked_add(food.dietary_energy_for_mass(*mass))
        })
        .unwrap_or_else(|| panic!("balanced meal offered energy overflowed"));

    assert!(
        offered >= required_offer || total == direct.maximum_meal_mass(),
        "balanced meal must cover its own eating-time metabolism unless the authored intake limit is the blocker"
    );
}
