//! Canonical conserved food and drink consumption transactions.

use crate::core::state::AppState;
use crate::core::time::TickSpan;

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

/// Reserves the survival-owner revision span required by one direct-consumption action.
///
/// Admission advances survival once, then every authoritative uptake tick advances it once more.
/// Eating and drinking share this exact continuation contract so neither path can under-reserve
/// trusted-load headroom independently.
pub(crate) fn direct_consumption_survival_revisions(
    state: &AppState,
    duration: TickSpan,
) -> Option<(u64, u64)> {
    let required_revisions = duration.value().checked_add(1)?;
    if !state.survival().can_advance_revision_by(required_revisions) {
        return None;
    }
    let expected_revision = state.survival().revision();
    let next_revision = expected_revision.checked_add(1)?;
    Some((expected_revision, next_revision))
}

#[cfg(test)]
#[path = "consumption_tests.rs"]
mod tests;
