//! Immutable survival definitions and their canonical registry.

mod intake;
mod physiology;
mod registry;

pub use intake::{
    ConsumptionTemperatureRange, DrinkDefinition, FoodCategory, FoodDefinition,
    calculate_food_hydration_offer,
};
pub use physiology::{
    DirectConsumptionDefinition, HydrationDefinition, MetabolismDefinition, NutritionDefinition,
    PhysiologyDefinition,
};
pub use registry::SurvivalRegistry;

#[cfg(all(
    test,
    any(not(feature = "test-unit-shard"), feature = "test-unit-survival")
))]
#[path = "definitions_tests.rs"]
mod tests;
