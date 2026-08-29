//! Canonical conserved food and drink consumption transactions.

mod absorption;
mod drinking;
mod eating;
mod freshness;

pub use drinking::{DrinkCommitError, DrinkError, DrinkOutcome, ValidatedDrink, validate_drink};
pub use eating::{
    EatCommitError, EatError, EatOutcome, EatPortionOutcome, NutritionGain, ValidatedEat,
    validate_eat,
};
pub use freshness::{FoodFreshness, FoodFreshnessError, assess_food_freshness};

pub(crate) use absorption::{DirectConsumptionInstallment, direct_consumption_installment};

#[cfg(test)]
#[path = "consumption_tests.rs"]
mod tests;
