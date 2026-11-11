//! Exact-lot food-use admission, reserve targets, and typed diagnostics.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Energy, Mass};
use crate::core::state::AppState;
use crate::inventory::{MaterialLotId, MaterialLotSelection};
use crate::logistics::validate_player_stockpile_access;
use crate::material::CommodityKey;
use crate::registry::Registries;
use crate::survival::Vitality;

use super::super::eating::project_minimum_meal_to_metabolic_target_for_available_source;
use super::super::{EatError, MealMetabolicProjectionError, ValidatedEat, validate_eat};
use super::{metabolic_energy_after_pending_locomotion, selected_food_dietary_refill_mass};

/// Failure while composing a selected physical food lot into an exact eating transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EatLotToTargetError {
    SurvivalNotInitialized,
    UnknownLot {
        lot: MaterialLotId,
    },
    NotEdible {
        commodity: CommodityKey,
    },
    Projection(MealMetabolicProjectionError),
    InsufficientLotMass {
        lot: MaterialLotId,
        available: Mass,
        required: Mass,
    },
    Eat(EatError),
}

impl Display for EatLotToTargetError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SurvivalNotInitialized => {
                formatter.write_str("selected-stack eating requires initialized player survival")
            }
            Self::UnknownLot { lot } => {
                write!(formatter, "unknown selected food lot {}", lot.value())
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
            Self::InsufficientLotMass {
                lot,
                available,
                required,
            } => write!(
                formatter,
                "selected food stack {} contains {} mg but {} mg is needed for the requested reserve target",
                lot.value(),
                available.milligrams(),
                required.milligrams()
            ),
            Self::Eat(error) => write!(formatter, "selected-stack eating failed: {error}"),
        }
    }
}

impl Error for EatLotToTargetError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Projection(error) => Some(error),
            Self::Eat(error) => Some(error),
            Self::SurvivalNotInitialized
            | Self::UnknownLot { .. }
            | Self::NotEdible { .. }
            | Self::InsufficientLotMass { .. } => None,
        }
    }
}

/// Familiar default food-use action: eat toward full metabolic and dietary reserves.
///
/// Reaching full reserve is a preference, not an admission requirement. If one legal meal cannot
/// reach full reserve, or the selected stack contains less than the exact refill amount, this
/// action consumes the largest legal available portion instead. A deficient selected food
/// category requests its deficit-sized serving whenever this is larger than the meal required
/// for metabolic energy, including when energy is already full. Explicit callers that require
/// a particular metabolic reserve should use
/// [`validate_eat_lot_to_metabolic_target`].
pub fn validate_eat_lot_to_full(
    registries: &Registries,
    state: &AppState,
    lot: MaterialLotId,
) -> Result<Option<ValidatedEat>, EatLotToTargetError> {
    let result = validate_eat_lot_to_metabolic_target(
        registries,
        state,
        lot,
        registries
            .survival()
            .physiology()
            .maximum_metabolic_energy(),
    );
    match result {
        Ok(Some(validated)) => match state.inventory().get_lot(lot).and_then(|record| {
            selected_food_dietary_refill_mass(registries, state, record.commodity())
        }) {
            Some(mass) if mass > validated.planned_mass() => {
                validate_eat_lot_up_to(registries, state, lot, mass)
            }
            _ => Ok(Some(validated)),
        },
        Ok(None) => match state.inventory().get_lot(lot).and_then(|record| {
            selected_food_dietary_refill_mass(registries, state, record.commodity())
        }) {
            Some(mass) => validate_eat_lot_up_to(registries, state, lot, mass),
            None => Ok(None),
        },
        Err(EatLotToTargetError::Projection(
            MealMetabolicProjectionError::TargetUnreachableWithinIntakeLimit { maximum_meal_mass },
        )) => validate_eat_lot_up_to(registries, state, lot, maximum_meal_mass),
        Err(EatLotToTargetError::InsufficientLotMass { available, .. }) => {
            validate_eat_lot_up_to(registries, state, lot, available)
        }
        result => result,
    }
}

fn validate_eat_lot_up_to(
    registries: &Registries,
    state: &AppState,
    lot: MaterialLotId,
    desired: Mass,
) -> Result<Option<ValidatedEat>, EatLotToTargetError> {
    let lot_record = state
        .inventory()
        .get_lot(lot)
        .ok_or(EatLotToTargetError::UnknownLot { lot })?;
    let source = lot_record.stockpile();
    validate_player_stockpile_access(state, source)
        .map_err(|error| EatLotToTargetError::Eat(EatError::Access(error)))?;
    let direct = registries.survival().physiology().direct_consumption();
    let available = lot_record.mass();
    let portion = desired.min(available).min(direct.maximum_meal_mass());
    if portion < direct.minimum_meal_mass() && portion != available {
        return Err(EatLotToTargetError::InsufficientLotMass {
            lot,
            available,
            required: direct.minimum_meal_mass(),
        });
    }
    validate_eat(
        registries,
        state,
        source,
        &[MaterialLotSelection::new(lot, portion)],
    )
    .map(Some)
    .map_err(EatLotToTargetError::Eat)
}

/// Validates eating the minimum amount from one selected stack needed to reach `target`.
///
/// `Ok(None)` means the player already satisfies the requested reserve. The caller chooses policy
/// (full, warning-safe, work reserve, and so on); this adapter only translates the familiar held
/// stack action into exact mass and then delegates freshness, temperature, attention, conservation,
/// and physiological legality to [`validate_eat`].
pub fn validate_eat_lot_to_metabolic_target(
    registries: &Registries,
    state: &AppState,
    lot: MaterialLotId,
    target: Energy,
) -> Result<Option<ValidatedEat>, EatLotToTargetError> {
    let player = state
        .survival()
        .player()
        .copied()
        .ok_or(EatLotToTargetError::SurvivalNotInitialized)?;
    if player.vitality() == Vitality::ZERO {
        return Err(EatLotToTargetError::Eat(EatError::PlayerDead));
    }
    let lot_record = state
        .inventory()
        .get_lot(lot)
        .ok_or(EatLotToTargetError::UnknownLot { lot })?;
    let source = lot_record.stockpile();
    validate_player_stockpile_access(state, source)
        .map_err(|error| EatLotToTargetError::Eat(EatError::Access(error)))?;
    let food = registries
        .survival()
        .get_food(lot_record.commodity())
        .copied()
        .ok_or(EatLotToTargetError::NotEdible {
            commodity: lot_record.commodity(),
        })?;
    let projected = project_minimum_meal_to_metabolic_target_for_available_source(
        registries.survival().physiology(),
        food,
        metabolic_energy_after_pending_locomotion(state, player.metabolic_energy()),
        target,
        lot_record.mass(),
    )
    .map_err(EatLotToTargetError::Projection)?;
    let Some(projected) = projected else {
        return Ok(None);
    };
    let required = projected.mass();
    if required > lot_record.mass() {
        return Err(EatLotToTargetError::InsufficientLotMass {
            lot,
            available: lot_record.mass(),
            required,
        });
    }
    validate_eat(
        registries,
        state,
        source,
        &[MaterialLotSelection::new(lot, required)],
    )
    .map(Some)
    .map_err(EatLotToTargetError::Eat)
}
