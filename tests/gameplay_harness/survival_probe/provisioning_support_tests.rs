//! Alignment contracts for survival meal policy planning.

use std::collections::BTreeMap;

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

#[test]
fn provisioning_plan_reobserves_live_food_after_canonical_consumption() {
    let registries = deep_hearth::content::build_registries();
    let direct = registries.survival().physiology().direct_consumption();
    let minimum = direct.minimum_meal_mass();
    let maximum = direct.maximum_meal_mass();

    // Select a bounded ordinary generated world where one non-leading food lot can be exhausted as
    // a sequence of legal meals while the leading remaining lot can still support replanning.
    // Generation data is used only to choose the fixture before actor admission; the assertion below
    // is explicitly about planning from the admitted runtime state afterward.
    let can_exhaust_in_legal_meals = |mass: Mass| {
        if mass < minimum {
            return false;
        }
        let meal_count = mass.milligrams().div_ceil(maximum.milligrams());
        mass.milligrams() >= meal_count.saturating_mul(minimum.milligrams())
    };
    let world = (1_u64..=256)
        .map(|seed| provisioning_world(&registries, seed))
        .find(|world| world.foods.len() >= 2)
        .unwrap_or_else(|| panic!("bounded survival generation produced no multi-food witness"));
    let drink_supply = provisioning_drink_supply(&registries, &world);
    let mut prepared = prepare_provisioning_world(&registries, &world, drink_supply);
    let observed = observed_provisioning_foods(&registries, &prepared.state, prepared.ambient_meal);
    let consumed = observed
        .iter()
        .skip(1)
        .copied()
        .find(|food| can_exhaust_in_legal_meals(food.mass))
        .unwrap_or_else(|| {
            panic!("live survival inventory produced no exhaustible replan witness")
        });
    let consumed_lot = consumed.lot;
    let initial_mass = consumed.mass;
    let mut consumed_mass = Mass::ZERO;
    while let Some(remaining) = prepared
        .state
        .inventory()
        .get_lot(consumed_lot)
        .map(|lot| lot.mass())
    {
        let remaining_meals = remaining.milligrams().div_ceil(maximum.milligrams());
        let portion_mg = if remaining_meals == 1 {
            remaining.milligrams()
        } else {
            maximum.milligrams().min(
                remaining.milligrams() - (remaining_meals - 1).saturating_mul(minimum.milligrams()),
            )
        };
        let portion = Mass::from_milligrams(portion_mg);
        assert!(portion >= minimum && portion <= maximum);
        let (meal, _elapsed) = execute_planned_meal(
            &registries,
            &mut prepared.state,
            prepared.ambient_meal,
            &[MaterialLotSelection::new(consumed_lot, portion)],
        );
        assert_eq!(meal.total_mass(), portion);
        consumed_mass = consumed_mass
            .checked_add(portion)
            .unwrap_or_else(|| panic!("survival replan consumed mass overflowed"));
    }
    assert_eq!(consumed_mass, initial_mass);
    assert!(
        prepared.state.inventory().get_lot(consumed_lot).is_none(),
        "canonical eating must remove the exhausted lot before replanning"
    );

    let replanned = provisioning_plan(
        &registries,
        &prepared,
        DietProvisioningPolicy::BalancedRecovery,
    );
    assert!(
        replanned
            .selections
            .iter()
            .all(|selection| selection.lot() != consumed_lot),
        "survival actor planning must not reuse a food lot that canonical gameplay already exhausted"
    );
    for selection in &replanned.selections {
        let available = prepared
            .state
            .inventory()
            .get_lot(selection.lot())
            .unwrap_or_else(|| panic!("replanned survival food lot disappeared"))
            .mass();
        assert!(
            selection.mass() <= available,
            "survival actor meal selection exceeded currently visible inventory"
        );
    }
}
