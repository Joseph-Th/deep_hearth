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

#[cfg(test)]
#[path = "definitions_tests.rs"]
mod tests;
