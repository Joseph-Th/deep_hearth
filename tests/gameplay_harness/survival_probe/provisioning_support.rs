//! Canonical direct-consumption execution and shared provisioning arithmetic.

use super::super::direct_consumption_timing::finish_direct_consumption_work;
use super::*;

pub(super) fn mass_for_target_energy(food: FoodDefinition, target: Energy) -> Mass {
    food.minimum_mass_for_dietary_energy(target)
        .unwrap_or_else(|| panic!("survival probe meal mass exceeds authoritative range"))
}

pub(super) fn recovery_drink_volume(
    registries: &Registries,
    drink: DrinkDefinition,
    current_hydration: Volume,
    context: &'static str,
) -> Volume {
    let physiology = registries.survival().physiology();
    match project_minimum_drink_to_hydration_target(
        physiology,
        drink,
        current_hydration,
        physiology.maximum_hydration(),
    ) {
        Ok(Some(projection)) => projection.volume(),
        Ok(None) => Volume::ZERO,
        Err(DrinkHydrationProjectionError::TargetUnreachableWithinIntakeLimit {
            maximum_drink_volume,
        }) => maximum_drink_volume,
        Err(error) => panic!("{context} drink projection failed: {error}"),
    }
}

pub(super) struct ProvisioningActionOutcome {
    pub(super) meal: EatOutcome,
    pub(super) drank_volume: Volume,
    pub(super) hydration_offered: Volume,
    pub(super) elapsed_ticks: u64,
    pub(super) action_order: &'static str,
}

pub(super) fn execute_planned_meal(
    registries: &Registries,
    state: &mut AppState,
    source: StockpileId,
    selections: &[MaterialLotSelection],
) -> (EatOutcome, u64) {
    let meal = validate_eat(registries, state, source, selections)
        .unwrap_or_else(|error| panic!("survival probe varied meal validation failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("survival probe varied meal commit failed: {error}"));
    let elapsed = finish_direct_consumption(registries, state, meal.completes_at());
    (meal, elapsed)
}

pub(super) fn execute_planned_drink(
    registries: &Registries,
    state: &mut AppState,
    source: FluidStoreId,
    volume: Volume,
) -> (DrinkOutcome, u64) {
    let drank = validate_drink(registries, state, source, volume)
        .unwrap_or_else(|error| panic!("survival probe drinking validation failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("survival probe drinking commit failed: {error}"));
    let elapsed = finish_direct_consumption(registries, state, drank.completes_at());
    (drank, elapsed)
}

pub(super) fn execute_provisioning_actions(
    registries: &Registries,
    state: &mut AppState,
    prepared: &PreparedProvisioningWorld,
    drink: DrinkDefinition,
    selections: &[MaterialLotSelection],
    drink_first: bool,
) -> ProvisioningActionOutcome {
    let (meal, drank_volume, hydration_offered, elapsed_ticks, action_order) = if drink_first {
        let current_hydration = assess_survival(registries, state)
            .unwrap_or_else(|| panic!("survival provisioning lost the player before drinking"))
            .hydration();
        let drink_volume = recovery_drink_volume(
            registries,
            drink,
            current_hydration,
            "survival drink-first provisioning",
        );
        if drink_volume.is_zero() {
            let (meal, meal_ticks) =
                execute_planned_meal(registries, state, prepared.ambient_meal, selections);
            (meal, Volume::ZERO, Volume::ZERO, meal_ticks, "eat-only")
        } else {
            let (drank, drink_ticks) =
                execute_planned_drink(registries, state, prepared.drink_store, drink_volume);
            let (meal, meal_ticks) =
                execute_planned_meal(registries, state, prepared.ambient_meal, selections);
            (
                meal,
                drank.volume(),
                drank.hydration_offered(),
                drink_ticks.checked_add(meal_ticks).unwrap_or_else(|| {
                    panic!("survival provisioning attention duration overflowed")
                }),
                "drink->eat",
            )
        }
    } else {
        let (meal, meal_ticks) =
            execute_planned_meal(registries, state, prepared.ambient_meal, selections);
        let current_hydration = assess_survival(registries, state)
            .unwrap_or_else(|| panic!("survival provisioning lost the player after eating"))
            .hydration();
        let drink_volume = recovery_drink_volume(
            registries,
            drink,
            current_hydration,
            "survival eat-first provisioning",
        );
        if drink_volume.is_zero() {
            (meal, Volume::ZERO, Volume::ZERO, meal_ticks, "eat-only")
        } else {
            let (drank, drink_ticks) =
                execute_planned_drink(registries, state, prepared.drink_store, drink_volume);
            (
                meal,
                drank.volume(),
                drank.hydration_offered(),
                meal_ticks.checked_add(drink_ticks).unwrap_or_else(|| {
                    panic!("survival provisioning attention duration overflowed")
                }),
                "eat->drink",
            )
        }
    };
    ProvisioningActionOutcome {
        meal,
        drank_volume,
        hydration_offered,
        elapsed_ticks,
        action_order,
    }
}

pub(super) struct LivedWaitOutcome {
    pub(super) drinks: u64,
    pub(super) drink_volume_ul: u64,
}

/// Advances the provisioning wait as lived time instead of idle depletion.
///
/// Full-reserve starts live long enough for canonical thirst to reach the authored warning
/// boundary mid-wait. The actor observes reserves on bounded legs and drinks through the same
/// canonical direct-consumption path as decision-point provisioning when that boundary is
/// reached, so the wait demonstrates reprovisioning under real pressure. Warning-boundary
/// starts are admitted at their decision point by construction and keep the single
/// uninterrupted wait.
pub(super) fn advance_lived_wait(
    registries: &Registries,
    state: &mut AppState,
    world: &ProvisioningWorld,
    drink_store: FluidStoreId,
    wait_ticks: u64,
) -> LivedWaitOutcome {
    const OBSERVATION_LEG_TICKS: u64 = 2_000;
    let mut drinks = 0_u64;
    let mut drink_volume_ul = 0_u64;
    let mut remaining = wait_ticks;
    while remaining > 0 {
        let leg = remaining.min(OBSERVATION_LEG_TICKS);
        advance_idle_ticks(registries, state, leg, "provisioning lived wait");
        remaining -= leg;
        if world.start_profile != SurvivalStartProfile::FullReserve {
            continue;
        }
        let physiology = registries.survival().physiology();
        let assessment = assess_survival(registries, state)
            .unwrap_or_else(|| panic!("survival lived wait lost the player"));
        if assessment.hydration() > physiology.thirsty_below() {
            continue;
        }
        let drink_volume = recovery_drink_volume(
            registries,
            world.drink,
            assessment.hydration(),
            "survival lived-wait recovery",
        );
        if drink_volume.is_zero() {
            continue;
        }
        let (drank, _) = execute_planned_drink(registries, state, drink_store, drink_volume);
        drinks = drinks
            .checked_add(1)
            .unwrap_or_else(|| panic!("survival lived-wait drink count overflowed"));
        drink_volume_ul = drink_volume_ul
            .checked_add(drank.volume().microliters())
            .unwrap_or_else(|| panic!("survival lived-wait drink volume overflowed"));
    }
    LivedWaitOutcome {
        drinks,
        drink_volume_ul,
    }
}

pub(super) fn finish_direct_consumption(
    registries: &Registries,
    state: &mut AppState,
    completes_at: SimulationTick,
) -> u64 {
    finish_direct_consumption_work(
        registries,
        state,
        completes_at,
        "survival direct consumption",
    )
}

pub(super) fn bound_meal_masses_to_direct_limit(masses: &[Mass], maximum: Mass) -> Vec<Mass> {
    assert!(!masses.is_empty(), "survival meal plan must select food");
    let total_milligrams = masses
        .iter()
        .try_fold(0_u128, |total, mass| {
            total.checked_add(u128::from(mass.milligrams()))
        })
        .unwrap_or_else(|| panic!("survival meal plan mass overflowed"));
    let maximum_milligrams = maximum.milligrams();
    if total_milligrams <= u128::from(maximum_milligrams) {
        return masses.to_vec();
    }

    let selected_count = u64::try_from(masses.len())
        .unwrap_or_else(|_| panic!("survival meal selection count exceeds u64"));
    assert!(
        maximum_milligrams >= selected_count,
        "authored direct meal limit must permit at least one milligram per selected category"
    );
    let mut allocated_milligrams = 0_u64;
    let mut bounded = Vec::with_capacity(masses.len());
    for (index, mass) in masses.iter().enumerate() {
        let remaining_slots = masses.len() - index - 1;
        let take = if remaining_slots == 0 {
            maximum_milligrams
                .checked_sub(allocated_milligrams)
                .unwrap_or_else(|| panic!("survival bounded meal allocation underflowed"))
        } else {
            let proportional = u128::from(mass.milligrams())
                .checked_mul(u128::from(maximum_milligrams))
                .unwrap_or_else(|| panic!("survival bounded meal scaling overflowed"))
                / total_milligrams;
            let proportional = u64::try_from(proportional)
                .unwrap_or_else(|_| panic!("survival bounded meal portion exceeds u64"))
                .max(1);
            let reserved_for_remaining = u64::try_from(remaining_slots)
                .unwrap_or_else(|_| panic!("survival meal selection count exceeds u64"));
            let maximum_here = maximum_milligrams
                .checked_sub(allocated_milligrams)
                .and_then(|remaining| remaining.checked_sub(reserved_for_remaining))
                .unwrap_or_else(|| panic!("survival bounded meal allocation exhausted early"));
            proportional.min(maximum_here)
        };
        allocated_milligrams = allocated_milligrams
            .checked_add(take)
            .unwrap_or_else(|| panic!("survival bounded meal allocation overflowed"));
        bounded.push(Mass::from_milligrams(take));
    }
    assert_eq!(allocated_milligrams, maximum_milligrams);
    bounded
}

pub(super) fn food_category_count(foods: &[FoodDefinition]) -> usize {
    foods
        .iter()
        .map(|food| food.category())
        .collect::<BTreeSet<_>>()
        .len()
}

pub(super) fn food_option_summary(registries: &Registries, foods: &[FoodDefinition]) -> String {
    foods
        .iter()
        .map(|food| {
            let material = registries
                .materials()
                .get_material(food.commodity().material())
                .unwrap_or_else(|| unreachable!("validated food option has a material"));
            format!(
                "{}:{}:{:?}:{}nJ/mg:{}ppm-hydration:{}t",
                food.commodity().value(),
                material.name(),
                food.category(),
                food.dietary_energy().nanojoules_per_milligram(),
                food.hydration_multiplier_ppm(),
                food.shelf_life().value(),
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}

pub(super) const fn category_salt(category: FoodCategory) -> u64 {
    match category {
        FoodCategory::Grain => 0x4752_4149_4E00_0001,
        FoodCategory::Fruit => 0x4652_5549_5400_0002,
        FoodCategory::Protein => 0x5052_4F54_4549_4E03,
    }
}

pub(super) fn normalized_deficit_priority(
    energy_deficit_ppm: u32,
    hydration_deficit_ppm: u32,
) -> ProvisioningPriority {
    match hydration_deficit_ppm.cmp(&energy_deficit_ppm) {
        std::cmp::Ordering::Greater => ProvisioningPriority::Hydration,
        std::cmp::Ordering::Less => ProvisioningPriority::MetabolicEnergy,
        std::cmp::Ordering::Equal => ProvisioningPriority::Balanced,
    }
}

pub(super) fn provisioning_drink_supply(
    registries: &Registries,
    world: &ProvisioningWorld,
) -> Volume {
    // Provision the world with enough finite drink to recover from any legal player reserve state.
    // One maximum-hydration supply covers both a full-volume lived-wait top-up and a
    // full-volume decision-point drink, so setup does not need to predict passive losses.
    // The acting plans below size each actual drink from authoritative assessments.
    world
        .drink
        .minimum_volume_for_hydration(registries.survival().physiology().maximum_hydration())
        .unwrap_or_else(|| panic!("survival probe drink supply exceeds authoritative range"))
}
