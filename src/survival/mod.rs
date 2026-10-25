//! Player survival, perishability, and conserved food/water consumption.

mod assessment;
mod consumption;
mod definitions;
mod lifecycle;
mod resource_cost;
mod state;
mod validation;

pub(crate) use consumption::freshness_from_history;

pub use assessment::{
    HungerState, HydrationState, SurvivalAssessment, ThermalComfortState, assess_survival,
};
pub use consumption::{
    AdjacentDrinkSource, DrinkCommitError, DrinkError, DrinkHydrationProjectionError, DrinkOutcome,
    DrinkStoreToTargetError, EatCommitError, EatError, EatLotToTargetError, EatOutcome,
    EatPortionOutcome, EatStackPortionsError, EatStackToTargetError, FoodFreshness,
    FoodFreshnessError, FoodFreshnessProjectionError, FoodStackFreshnessError, FoodStackPortion,
    LocalDrinkSource, MealMetabolicProjectionError, MinimumDrinkHydrationProjection,
    MinimumMealMetabolicProjection, NutritionGain, ValidatedDrink, ValidatedEat,
    assess_food_freshness, assess_food_stack_freshness, available_adjacent_drink_sources,
    available_local_drink_sources, project_food_freshness_after_storage_transition,
    project_minimum_drink_to_hydration_target, project_minimum_meal_to_metabolic_target,
    validate_drink, validate_drink_store_to_full, validate_drink_store_to_hydration_target,
    validate_eat, validate_eat_lot_to_full, validate_eat_lot_to_metabolic_target,
    validate_eat_stack_portions, validate_eat_stack_to_full,
    validate_eat_stack_to_metabolic_target,
};
pub use definitions::{
    BodyKinematicsDefinition, BodyStanceDefinition, BodyViewDefinition,
    ConsumptionTemperatureRange, DirectConsumptionDefinition, DrinkDefinition, FoodCategory,
    FoodDefinition, HydrationDefinition, LocomotionDefinition, LocomotionSpeedDefinition,
    MetabolismDefinition, NutritionDefinition, PhysiologyDefinition, SurvivalRegistry,
    SurvivalStressDefinition, ThermalComfortDefinition, ThermalExposure, WalkingSpeedDefinition,
    calculate_food_hydration_offer,
};
pub use resource_cost::{
    SurvivalExertion, SurvivalResourceBudget, SurvivalResourceProjectionError,
    project_survival_resource_budget,
};
pub use state::{
    NUTRITION_PARTS_PER_MILLION, NutritionReserves, PlayerSurvivalRecord, SurvivalState, Vitality,
};
pub use validation::SurvivalValidationError;

#[cfg(any(test, feature = "test-gameplay"))]
pub(crate) use lifecycle::initialize_player_survival;
#[cfg(feature = "test-gameplay")]
pub(crate) use lifecycle::validate_player_survival_initialization_with_reserves;
pub(crate) use lifecycle::{
    InitializeSurvivalError, MeleeExertionError, PlayerSurvivalInitializationCommitError,
    RespawnSurvivalError, SurvivalTickError, SurvivalTickPlan,
    ValidatedPlayerSurvivalInitialization, ValidatedPlayerSurvivalRespawn, apply_survival_tick,
    decide_control_exertion_settlement, decide_melee_exertion,
    decide_survival_tick_with_local_heat, validate_player_survival_initialization,
    validate_player_survival_respawn,
};
#[cfg(feature = "test-gameplay")]
pub(crate) use lifecycle::{
    initialize_player_survival_at_hunger_warning_boundary_for_fixture,
    initialize_player_survival_at_hydration_warning_boundary_for_fixture,
};
pub(crate) use resource_cost::{
    SurvivalTickResourceCostError, control_exertion_between, resolve_survival_tick_resource_cost,
};
pub(crate) use state::PendingDirectConsumption;
#[cfg(test)]
pub(crate) use state::{player_record, player_record_with_thermal_comfort};
pub(crate) use validation::validate_loaded_survival;
