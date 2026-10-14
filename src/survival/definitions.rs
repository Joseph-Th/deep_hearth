//! Immutable survival definitions and their canonical registry.

mod intake;
mod physiology;
mod registry;

pub use intake::{
    ConsumptionTemperatureRange, DrinkDefinition, FoodCategory, FoodDefinition,
    calculate_food_hydration_offer,
};
pub use physiology::{
    BodyKinematicsDefinition, BodyStanceDefinition, BodyViewDefinition,
    DirectConsumptionDefinition, HydrationDefinition, LocomotionDefinition,
    LocomotionSpeedDefinition, MetabolismDefinition, NutritionDefinition, PhysiologyDefinition,
    SurvivalStressDefinition, ThermalComfortDefinition,
};
pub use registry::SurvivalRegistry;

#[cfg(test)]
#[path = "definitions_tests.rs"]
mod tests;
