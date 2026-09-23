//! Canonical conserved food and drink consumption transactions.

mod absorption;
mod drinking;
mod eating;
mod freshness;

pub use drinking::{
    DrinkCommitError, DrinkError, DrinkHydrationProjectionError, DrinkOutcome,
    MinimumDrinkHydrationProjection, ValidatedDrink, project_minimum_drink_to_hydration_target,
    validate_drink,
};
pub use eating::{
    EatCommitError, EatError, EatOutcome, EatPortionOutcome, MealMetabolicProjectionError,
    MinimumMealMetabolicProjection, NutritionGain, ValidatedEat,
    project_minimum_meal_to_metabolic_target, validate_eat,
};
pub(crate) use freshness::freshness_from_history;
pub use freshness::{
    FoodFreshness, FoodFreshnessError, FoodFreshnessProjectionError, assess_food_freshness,
    project_food_freshness_after_storage_transition,
};

pub(crate) use absorption::{DirectConsumptionInstallment, direct_consumption_installment};

#[cfg(test)]
#[path = "consumption_tests.rs"]
mod tests;
