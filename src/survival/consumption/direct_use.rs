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

use super::{EatError, MealMetabolicProjectionError, ValidatedEat};

mod drink;
mod portions;
use super::eating::{
    project_minimum_meal_to_metabolic_target_for_available_source, validate_eat_with_mode,
};
pub use drink::{
    DrinkStoreToTargetError, validate_drink_store_to_full, validate_drink_store_to_hydration_target,
};
pub(crate) use drink::{
    assess_drink_store_to_full_admission, validate_drink_store_to_full_control_rate,
};
pub use portions::{EatStackPortionsError, FoodStackPortion, validate_eat_stack_portions};

/// Failure while composing one familiar projected food stack into exact eating.
/// Inventory lot identity is kept private.
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

/// Portion toward the highest dietary reserve attainable after tick-end decay.
fn selected_food_dietary_refill_mass(
    registries: &Registries,
    state: &AppState,
    stack: &MaterialStackProjection,
) -> Option<Mass> {
    let food = registries.survival().get_food(stack.commodity())?;
    let player = state.survival().player()?;
    let physiology = registries.survival().physiology();
    let nutrition = physiology.nutrition();
    let direct = physiology.direct_consumption();
    let full = crate::survival::NUTRITION_PARTS_PER_MILLION;
    let decay = nutrition.decay_ppm_per_tick();
    let current = player.nutrition().get(food.category());
    // Intake is capped before decay. Immediately after a successful meal,
    // this unavoidable one-tick deficit must not trigger another serving.
    if current >= full - decay {
        return None;
    }
    let missing = u128::from(full - current);
    let full_mass = u128::from(nutrition.full_reserve_food_mass().milligrams());
    let maximum = stack.mass().min(direct.maximum_meal_mass());
    let minimum = direct.minimum_meal_mass().min(maximum);
    // Nutrition offers above one full reserve are capped by the intake owner.
    let limit = maximum.min(nutrition.full_reserve_food_mass().max(minimum));
    let mass_for_gain = |gain: u128| {
        Mass::from_milligrams(
            u64::try_from((gain.min(u128::from(full)) * full_mass).div_ceil(u128::from(full)))
                .expect("bounded dietary offer fits authored full-reserve mass"),
        )
        .max(minimum)
        .min(limit)
    };
    let mut portion = mass_for_gain(missing);
    loop {
        if portion == limit {
            return Some(portion);
        }
        let duration = direct.meal_duration(portion)?;
        // The final decay determines the attainable ceiling; all earlier
        // decays must be replaced by the offered nutrition.
        let gain = missing + u128::from(decay) * u128::from(duration.value() - 1);
        let next = mass_for_gain(gain);
        if next == portion {
            return Some(portion);
        }
        debug_assert!(next > portion);
        portion = next;
    }
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
    let physiology = registries.survival().physiology();
    let metabolic_mass = match project_eat_stack_to_metabolic_target(
        registries,
        state,
        stack,
        physiology.maximum_metabolic_energy(),
    ) {
        Ok(mass) => mass.unwrap_or(Mass::ZERO),
        Err(EatStackToTargetError::Projection(
            MealMetabolicProjectionError::TargetUnreachableWithinIntakeLimit { maximum_meal_mass },
        )) => maximum_meal_mass,
        Err(EatStackToTargetError::InsufficientStackMass { available, .. }) => available,
        Err(error) => return Err(error),
    };
    let dietary_mass =
        selected_food_dietary_refill_mass(registries, state, stack).unwrap_or(Mass::ZERO);
    let portion = metabolic_mass
        .max(dietary_mass)
        .min(stack.mass())
        .min(physiology.direct_consumption().maximum_meal_mass());
    if portion.is_zero() {
        return Ok(None);
    }
    validate_eat_stack_portion(registries, state, stack, portion, preview_off_boundary).map(Some)
}

fn validate_eat_stack_portion(
    registries: &Registries,
    state: &AppState,
    stack: &MaterialStackProjection,
    portion: Mass,
    preview_off_boundary: bool,
) -> Result<ValidatedEat, EatStackToTargetError> {
    let source = stack.stockpile();
    let selections = stack
        .selections_for_mass(state, source, portion)
        .map_err(EatStackToTargetError::Stack)?;
    validate_eat_with_mode(registries, state, source, &selections, preview_off_boundary)
        .map_err(EatStackToTargetError::Eat)
}

/// Validates the minimum meal from one projected stack needed to reach `target`.
pub fn validate_eat_stack_to_metabolic_target(
    registries: &Registries,
    state: &AppState,
    stack: &MaterialStackProjection,
    target: Energy,
) -> Result<Option<ValidatedEat>, EatStackToTargetError> {
    project_eat_stack_to_metabolic_target(registries, state, stack, target)?
        .map(|portion| validate_eat_stack_portion(registries, state, stack, portion, false))
        .transpose()
}

/// Resolve the desired portion before admitting any exact consumption transaction.
fn project_eat_stack_to_metabolic_target(
    registries: &Registries,
    state: &AppState,
    stack: &MaterialStackProjection,
    target: Energy,
) -> Result<Option<Mass>, EatStackToTargetError> {
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
    let pending_locomotion = state.logistics().player().map_or(Energy::ZERO, |player| {
        player
            .locomotion_exertion()
            .pending()
            .energy_cost_per_tick()
    });
    let projected = project_minimum_meal_to_metabolic_target_for_available_source(
        registries.survival().physiology(),
        food,
        player.metabolic_energy(),
        target,
        pending_locomotion,
        stack.mass(),
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
    Ok(Some(required))
}
