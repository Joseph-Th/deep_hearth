//! Projected food-stack reserve targets over canonical conserved eating.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Energy, Mass};
use crate::core::state::AppState;
use crate::inventory::{MaterialStackProjection, MaterialStackProjectionError};
use crate::logistics::validate_player_stockpile_access;
use crate::material::CommodityKey;
use crate::registry::Registries;
use crate::survival::Vitality;

use super::{
    EatError, MealMetabolicProjectionError, ValidatedEat, project_minimum_meal_to_metabolic_target,
};

mod drink;
mod lot;
mod portions;
use super::eating::validate_eat_with_mode;
pub use drink::{
    DrinkStoreToTargetError, validate_drink_store_to_full, validate_drink_store_to_hydration_target,
};
pub(crate) use drink::{
    assess_drink_store_to_full_admission, validate_drink_store_to_full_control_rate,
};
pub use lot::{
    EatLotToTargetError, validate_eat_lot_to_full, validate_eat_lot_to_metabolic_target,
};
pub use portions::{EatStackPortionsError, FoodStackPortion, validate_eat_stack_portions};

/// Failure while composing one familiar projected food stack into exact eating.
///
/// Unlike the lot-oriented adapter, this surface does not expose inventory lot identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EatStackToTargetError {
    SurvivalNotInitialized,
    NotEdible { commodity: CommodityKey },
    Projection(MealMetabolicProjectionError),
    InsufficientStackMass { available: Mass, required: Mass },
    Stack(MaterialStackProjectionError),
    Eat(EatError),
}

impl Display for EatStackToTargetError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SurvivalNotInitialized => {
                formatter.write_str("selected-stack eating requires initialized player survival")
            }
            Self::NotEdible { commodity } => write!(
                formatter,
                "selected material {} form {} is not edible",
                commodity.material().value(),
                commodity.form().value()
            ),
            Self::Projection(error) => {
                write!(formatter, "selected-stack meal projection failed: {error}")
            }
            Self::InsufficientStackMass {
                available,
                required,
            } => write!(
                formatter,
                "selected food stack contains {} mg but {} mg is needed for the requested reserve target",
                available.milligrams(),
                required.milligrams()
            ),
            Self::Stack(error) => write!(formatter, "selected food stack became stale: {error}"),
            Self::Eat(error) => write!(formatter, "selected-stack eating failed: {error}"),
        }
    }
}

impl Error for EatStackToTargetError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Projection(error) => Some(error),
            Self::Stack(error) => Some(error),
            Self::Eat(error) => Some(error),
            Self::SurvivalNotInitialized
            | Self::NotEdible { .. }
            | Self::InsufficientStackMass { .. } => None,
        }
    }
}

/// Minimum represented portion for the selected food's outstanding dietary reserve.
fn selected_food_dietary_refill_mass(
    registries: &Registries,
    state: &AppState,
    commodity: CommodityKey,
) -> Option<Mass> {
    let food = registries.survival().get_food(commodity)?;
    let player = state.survival().player()?;
    let missing_ppm = crate::survival::NUTRITION_PARTS_PER_MILLION
        .checked_sub(player.nutrition().get(food.category()))?;
    if missing_ppm == 0 {
        return None;
    }
    let physiology = registries.survival().physiology();
    let numerator = u128::from(physiology.nutrition().full_reserve_food_mass().milligrams())
        * u128::from(missing_ppm);
    let required = numerator
        .div_ceil(u128::from(crate::survival::NUTRITION_PARTS_PER_MILLION))
        .max(u128::from(
            physiology
                .direct_consumption()
                .minimum_meal_mass()
                .milligrams(),
        ));
    Some(Mass::from_milligrams(
        u64::try_from(required)
            .unwrap_or_else(|_| unreachable!("dietary refill cannot exceed full-reserve mass")),
    ))
}

/// Familiar stack-level food action from exactly the selected physical cohort.
///
/// The stack projection privately resolves current lot slices. This lets a UI distinguish two
/// freshness/temperature cohorts without exposing lot IDs. The familiar full-use preference
/// covers both metabolic replenishment and any larger deficit in the selected food category.
pub fn validate_eat_stack_to_full(
    registries: &Registries,
    state: &AppState,
    stack: &MaterialStackProjection,
) -> Result<Option<ValidatedEat>, EatStackToTargetError> {
    validate_eat_stack_to_full_with_mode(registries, state, stack, false)
}

/// Admit selected food on the input control frame. It still uses the exact
/// inventory, survival, and player-attention transaction.
pub(crate) fn validate_eat_stack_to_full_control_rate(
    registries: &Registries,
    state: &AppState,
    stack: &MaterialStackProjection,
) -> Result<Option<ValidatedEat>, EatStackToTargetError> {
    validate_eat_stack_to_full_with_mode(registries, state, stack, true)
}

fn validate_eat_stack_to_full_with_mode(
    registries: &Registries,
    state: &AppState,
    stack: &MaterialStackProjection,
    preview_off_boundary: bool,
) -> Result<Option<ValidatedEat>, EatStackToTargetError> {
    let result = validate_eat_stack_to_metabolic_target_with_mode(
        registries,
        state,
        stack,
        registries
            .survival()
            .physiology()
            .maximum_metabolic_energy(),
        preview_off_boundary,
    );
    match result {
        Ok(Some(validated)) => {
            match selected_food_dietary_refill_mass(registries, state, stack.commodity()) {
                Some(mass) if mass > validated.planned_mass() => {
                    validate_eat_stack_up_to(registries, state, stack, mass, preview_off_boundary)
                }
                _ => Ok(Some(validated)),
            }
        }
        Ok(None) => match selected_food_dietary_refill_mass(registries, state, stack.commodity()) {
            Some(mass) => {
                validate_eat_stack_up_to(registries, state, stack, mass, preview_off_boundary)
            }
            None => Ok(None),
        },
        Err(EatStackToTargetError::Projection(
            MealMetabolicProjectionError::TargetUnreachableWithinIntakeLimit { maximum_meal_mass },
        )) => validate_eat_stack_up_to(
            registries,
            state,
            stack,
            maximum_meal_mass,
            preview_off_boundary,
        ),
        Err(EatStackToTargetError::InsufficientStackMass { available, .. }) => {
            validate_eat_stack_up_to(registries, state, stack, available, preview_off_boundary)
        }
        result => result,
    }
}

fn validate_eat_stack_up_to(
    registries: &Registries,
    state: &AppState,
    stack: &MaterialStackProjection,
    desired: Mass,
    preview_off_boundary: bool,
) -> Result<Option<ValidatedEat>, EatStackToTargetError> {
    let source = stack.stockpile();
    validate_player_stockpile_access(state, source)
        .map_err(|error| EatStackToTargetError::Eat(EatError::Access(error)))?;
    let direct = registries.survival().physiology().direct_consumption();
    let available = stack.mass();
    let portion = desired.min(available).min(direct.maximum_meal_mass());
    if portion < direct.minimum_meal_mass() {
        return Err(EatStackToTargetError::InsufficientStackMass {
            available,
            required: direct.minimum_meal_mass(),
        });
    }
    let selections = stack
        .selections_for_mass(state, source, portion)
        .map_err(EatStackToTargetError::Stack)?;
    validate_eat_with_mode(registries, state, source, &selections, preview_off_boundary)
        .map(Some)
        .map_err(EatStackToTargetError::Eat)
}

/// Validates the minimum meal from one projected stack needed to reach `target`.
pub fn validate_eat_stack_to_metabolic_target(
    registries: &Registries,
    state: &AppState,
    stack: &MaterialStackProjection,
    target: Energy,
) -> Result<Option<ValidatedEat>, EatStackToTargetError> {
    validate_eat_stack_to_metabolic_target_with_mode(registries, state, stack, target, false)
}

fn validate_eat_stack_to_metabolic_target_with_mode(
    registries: &Registries,
    state: &AppState,
    stack: &MaterialStackProjection,
    target: Energy,
    preview_off_boundary: bool,
) -> Result<Option<ValidatedEat>, EatStackToTargetError> {
    let player = state
        .survival()
        .player()
        .copied()
        .ok_or(EatStackToTargetError::SurvivalNotInitialized)?;
    // Even a satisfied reserve must not turn a dead actor's use into an
    // apparently successful no-op. Canonical eating rejects the same state.
    if player.vitality() == Vitality::ZERO {
        return Err(EatStackToTargetError::Eat(EatError::PlayerDead));
    }
    let source = stack.stockpile();
    validate_player_stockpile_access(state, source)
        .map_err(|error| EatStackToTargetError::Eat(EatError::Access(error)))?;
    // A no-op refill target does not make an outdated display selection valid.
    stack
        .validate_current(state)
        .map_err(EatStackToTargetError::Stack)?;
    let food = registries
        .survival()
        .get_food(stack.commodity())
        .copied()
        .ok_or(EatStackToTargetError::NotEdible {
            commodity: stack.commodity(),
        })?;
    // A selected food action can occur between the movement input and the
    // slow survival settlement that charges it. The displayed metabolic
    // reserve still includes that already-incurred cost, so discount the
    // debt before sizing a meal toward its requested post-intake target.
    let pending_locomotion = state
        .logistics()
        .player()
        .map_or(Energy::ZERO, |logistics| {
            logistics
                .locomotion_exertion()
                .pending()
                .energy_cost_per_tick()
        });
    let effective_metabolic_energy = player
        .metabolic_energy()
        .checked_sub(pending_locomotion)
        .unwrap_or(Energy::ZERO);
    let projected = project_minimum_meal_to_metabolic_target(
        registries.survival().physiology(),
        food,
        effective_metabolic_energy,
        target,
    )
    .map_err(EatStackToTargetError::Projection)?;
    let Some(projected) = projected else {
        return Ok(None);
    };
    let required = projected.mass();
    if required > stack.mass() {
        return Err(EatStackToTargetError::InsufficientStackMass {
            available: stack.mass(),
            required,
        });
    }
    let selections = stack
        .selections_for_mass(state, source, required)
        .map_err(EatStackToTargetError::Stack)?;
    validate_eat_with_mode(registries, state, source, &selections, preview_off_boundary)
        .map(Some)
        .map_err(EatStackToTargetError::Eat)
}
