//! Player survival, perishability, and conserved food/water consumption.

mod consumption;
mod definitions;
mod lifecycle;
mod state;
mod validation;

pub use consumption::{
    DrinkCommitError, DrinkError, DrinkOutcome, EatCommitError, EatError, EatOutcome,
    FoodFreshness, FoodFreshnessError, ValidatedDrink, ValidatedEat, assess_food_freshness,
    survival_after_action, validate_drink, validate_eat,
};
pub use definitions::{
    DrinkDefinition, FoodCategory, FoodDefinition, HydrationDefinition, MetabolismDefinition,
    PhysiologyDefinition, SurvivalRegistry,
};
pub use lifecycle::{
    HungerState, HydrationState, InitializeSurvivalError, SurvivalAssessment, assess_survival,
    initialize_player_survival,
};
pub use state::{PlayerSurvivalRecord, SurvivalState, Vitality};
pub use validation::SurvivalValidationError;

pub(crate) use lifecycle::{SurvivalTickError, apply_survival_tick, decide_survival_tick};
pub(crate) use validation::validate_loaded_survival;
