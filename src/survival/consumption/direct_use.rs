//! Familiar selected-stack/vessel consumption actions over exact survival transactions.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Energy, Mass, Volume};
use crate::core::state::AppState;
use crate::fluid::FluidStoreId;
use crate::inventory::{MaterialLotId, MaterialLotSelection};
use crate::material::CommodityKey;
use crate::registry::Registries;

use super::{
    DrinkError, DrinkHydrationProjectionError, EatError, MealMetabolicProjectionError,
    ValidatedDrink, ValidatedEat, project_minimum_drink_to_hydration_target,
    project_minimum_meal_to_metabolic_target, validate_drink, validate_eat,
};

/// Failure while composing a selected food stack into the exact eating transaction.
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

/// Failure while composing one selected fluid vessel/store into exact drinking.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DrinkStoreToTargetError {
    SurvivalNotInitialized,
    UnknownStore {
        store: FluidStoreId,
    },
    EmptyStore {
        store: FluidStoreId,
    },
    NotDrinkable {
        store: FluidStoreId,
    },
    Projection(DrinkHydrationProjectionError),
    InsufficientVolume {
        store: FluidStoreId,
        available: Volume,
        required: Volume,
    },
    Drink(DrinkError),
}

impl Display for DrinkStoreToTargetError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SurvivalNotInitialized => {
                formatter.write_str("selected-vessel drinking requires initialized player survival")
            }
            Self::UnknownStore { store } => {
                write!(formatter, "unknown selected fluid store {}", store.value())
            }
            Self::EmptyStore { store } => {
                write!(formatter, "selected fluid store {} is empty", store.value())
            }
            Self::NotDrinkable { store } => write!(
                formatter,
                "selected fluid store {} does not contain an authored drink",
                store.value()
            ),
            Self::Projection(error) => write!(
                formatter,
                "selected-vessel drink projection failed: {error}"
            ),
            Self::InsufficientVolume {
                store,
                available,
                required,
            } => write!(
                formatter,
                "selected fluid store {} contains {} uL but {} uL is needed for the requested hydration target",
                store.value(),
                available.microliters(),
                required.microliters()
            ),
            Self::Drink(error) => write!(formatter, "selected-vessel drinking failed: {error}"),
        }
    }
}

impl Error for DrinkStoreToTargetError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Projection(error) => Some(error),
            Self::Drink(error) => Some(error),
            Self::SurvivalNotInitialized
            | Self::UnknownStore { .. }
            | Self::EmptyStore { .. }
            | Self::NotDrinkable { .. }
            | Self::InsufficientVolume { .. } => None,
        }
    }
}

/// Familiar default food-use action: eat the minimum legal portion toward full metabolic reserve.
pub fn validate_eat_lot_to_full(
    registries: &Registries,
    state: &AppState,
    lot: MaterialLotId,
) -> Result<Option<ValidatedEat>, EatLotToTargetError> {
    validate_eat_lot_to_metabolic_target(
        registries,
        state,
        lot,
        registries
            .survival()
            .physiology()
            .maximum_metabolic_energy(),
    )
}

/// Familiar default drink-use action: drink the minimum legal portion toward full hydration.
pub fn validate_drink_store_to_full(
    registries: &Registries,
    state: &AppState,
    store: FluidStoreId,
) -> Result<Option<ValidatedDrink>, DrinkStoreToTargetError> {
    validate_drink_store_to_hydration_target(
        registries,
        state,
        store,
        registries.survival().physiology().maximum_hydration(),
    )
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
    let lot_record = state
        .inventory()
        .get_lot(lot)
        .ok_or(EatLotToTargetError::UnknownLot { lot })?;
    let food = registries
        .survival()
        .get_food(lot_record.commodity())
        .copied()
        .ok_or(EatLotToTargetError::NotEdible {
            commodity: lot_record.commodity(),
        })?;
    let projected = project_minimum_meal_to_metabolic_target(
        registries.survival().physiology(),
        food,
        player.metabolic_energy(),
        target,
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
    let source = lot_record.stockpile();
    validate_eat(
        registries,
        state,
        source,
        &[MaterialLotSelection::new(lot, required)],
    )
    .map(Some)
    .map_err(EatLotToTargetError::Eat)
}

/// Validates drinking the minimum amount from one selected vessel/store needed to reach `target`.
///
/// As with selected-stack eating, caller policy chooses the target while the existing drinking
/// owner remains authoritative for temperature, attention, finite withdrawal, and conservation.
pub fn validate_drink_store_to_hydration_target(
    registries: &Registries,
    state: &AppState,
    store: FluidStoreId,
    target: Volume,
) -> Result<Option<ValidatedDrink>, DrinkStoreToTargetError> {
    let player = state
        .survival()
        .player()
        .copied()
        .ok_or(DrinkStoreToTargetError::SurvivalNotInitialized)?;
    let store_record = state
        .fluid()
        .get_store(store)
        .ok_or(DrinkStoreToTargetError::UnknownStore { store })?;
    let contents = store_record
        .contents()
        .ok_or(DrinkStoreToTargetError::EmptyStore { store })?;
    let drink = registries
        .survival()
        .get_drink(contents.fluid())
        .copied()
        .ok_or(DrinkStoreToTargetError::NotDrinkable { store })?;
    let projected = project_minimum_drink_to_hydration_target(
        registries.survival().physiology(),
        drink,
        player.hydration(),
        target,
    )
    .map_err(DrinkStoreToTargetError::Projection)?;
    let Some(projected) = projected else {
        return Ok(None);
    };
    let required = projected.volume();
    if required > contents.volume() {
        return Err(DrinkStoreToTargetError::InsufficientVolume {
            store,
            available: contents.volume(),
            required,
        });
    }
    validate_drink(registries, state, store, required)
        .map(Some)
        .map_err(DrinkStoreToTargetError::Drink)
}
