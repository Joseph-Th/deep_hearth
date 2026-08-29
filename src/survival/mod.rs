//! Player survival, perishability, and conserved food/water consumption.

mod consumption;
mod definitions;
mod lifecycle;
mod state;
mod validation;

pub use consumption::{
    DrinkCommitError, DrinkError, DrinkOutcome, EatCommitError, EatError, EatOutcome,
    EatPortionOutcome, FoodFreshness, FoodFreshnessError, NutritionGain, ValidatedDrink,
    ValidatedEat, assess_food_freshness, validate_drink, validate_eat,
};
pub use definitions::{
    ConsumptionTemperatureRange, DirectConsumptionDefinition, DrinkDefinition, FoodCategory,
    FoodDefinition, HydrationDefinition, MetabolismDefinition, NutritionDefinition,
    PhysiologyDefinition, SurvivalRegistry,
};
pub use lifecycle::{
    HungerState, HydrationState, InitializeSurvivalError, SurvivalAssessment, SurvivalExertion,
    assess_survival, initialize_player_survival,
};
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
#[cfg(test)]
pub(crate) use state::player_record;
pub(crate) use validation::validate_loaded_survival;
