//! Player survival, perishability, and conserved food/water consumption.

mod assessment;
mod consumption;
mod definitions;
mod lifecycle;
mod resource_cost;
mod state;
mod validation;

pub use assessment::{HungerState, HydrationState, SurvivalAssessment, assess_survival};
pub use consumption::{
    DrinkCommitError, DrinkError, DrinkOutcome, EatCommitError, EatError, EatOutcome,
    EatPortionOutcome, FoodFreshness, FoodFreshnessError, FoodFreshnessProjectionError,
    NutritionGain, ValidatedDrink, ValidatedEat, assess_food_freshness,
    project_food_freshness_after_storage_transition, validate_drink, validate_eat,
};
pub use definitions::{
    ConsumptionTemperatureRange, DirectConsumptionDefinition, DrinkDefinition, FoodCategory,
    FoodDefinition, HydrationDefinition, MetabolismDefinition, NutritionDefinition,
    PhysiologyDefinition, SurvivalRegistry, calculate_food_hydration_offer,
};
pub use lifecycle::{InitializeSurvivalError, initialize_player_survival};
pub use resource_cost::SurvivalExertion;
pub use state::{
    NUTRITION_PARTS_PER_MILLION, NutritionReserves, PlayerSurvivalRecord, SurvivalState, Vitality,
};
pub use validation::SurvivalValidationError;

pub(crate) use lifecycle::{SurvivalTickError, apply_survival_tick, decide_survival_tick};
#[cfg(feature = "test-gameplay")]
pub(crate) use lifecycle::{
    initialize_player_survival_at_hunger_warning_boundary_for_fixture,
    initialize_player_survival_at_hydration_warning_boundary_for_fixture,
};
pub(crate) use resource_cost::{
    SurvivalTickResourceCostError, resolve_survival_tick_resource_cost,
};
pub(crate) use state::PendingDirectConsumption;
#[cfg(test)]
pub(crate) use state::player_record;
pub(crate) use validation::validate_loaded_survival;
