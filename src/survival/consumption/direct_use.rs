//! Familiar selected-stack/vessel consumption actions over exact survival transactions.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Energy, Mass, Volume};
use crate::core::state::AppState;
use crate::fluid::FluidStoreId;
use crate::inventory::{
    MaterialLotId, MaterialLotSelection, MaterialStackProjection, MaterialStackProjectionError,
};
use crate::logistics::{validate_player_fluid_store_access, validate_player_stockpile_access};
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

#[must_use]
#[derive(Clone, Copy, Debug)]
pub struct FoodStackPortion<'a> {
    stack: &'a MaterialStackProjection,
    mass: Mass,
}

impl<'a> FoodStackPortion<'a> {
    pub const fn new(stack: &'a MaterialStackProjection, mass: Mass) -> Self {
        Self { stack, mass }
    }

    pub const fn stack(self) -> &'a MaterialStackProjection {
        self.stack
    }

    #[must_use]
    pub const fn mass(self) -> Mass {
        self.mass
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EatStackPortionsError {
    EmptyMeal,
    MixedSources {
        expected: crate::inventory::StockpileId,
        actual: crate::inventory::StockpileId,
    },
    Stack {
        portion: usize,
        error: MaterialStackProjectionError,
    },
    Eat(EatError),
}

impl Display for EatStackPortionsError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyMeal => {
                formatter.write_str("visible-stack meal requires at least one portion")
            }
            Self::MixedSources { expected, actual } => write!(
                formatter,
                "visible-stack meal mixes stockpiles {} and {}",
                expected.value(),
                actual.value()
            ),
            Self::Stack { portion, error } => {
                write!(formatter, "meal portion {portion} became stale: {error}")
            }
            Self::Eat(error) => write!(formatter, "visible-stack meal failed: {error}"),
        }
    }
}

impl Error for EatStackPortionsError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Stack { error, .. } => Some(error),
            Self::Eat(error) => Some(error),
            Self::EmptyMeal | Self::MixedSources { .. } => None,
        }
    }
}

/// Validates one deliberate meal composed from visible physical stack portions.
///
/// Exact lot membership remains private to inventory. Every portion must come from the same
/// stockpile; canonical eating still owns freshness, intake limits, nutrition, timing, attention,
/// custody, and conservation.
pub fn validate_eat_stack_portions(
    registries: &Registries,
    state: &AppState,
    portions: &[FoodStackPortion<'_>],
) -> Result<ValidatedEat, EatStackPortionsError> {
    let source = portions
        .first()
        .map(|portion| portion.stack().stockpile())
        .ok_or(EatStackPortionsError::EmptyMeal)?;
    // Portions can reference the same physical lot. Normalize their exact slices before
    // admission; the canonical validator then checks combined mass, custody, and legality.
    let mut mass_by_lot = BTreeMap::<MaterialLotId, u128>::new();
    for (index, portion) in portions.iter().copied().enumerate() {
        if portion.stack().stockpile() != source {
            return Err(EatStackPortionsError::MixedSources {
                expected: source,
                actual: portion.stack().stockpile(),
            });
        }
        for selection in portion
            .stack()
            .selections_for_mass(state, source, portion.mass())
            .map_err(|error| EatStackPortionsError::Stack {
                portion: index,
                error,
            })?
        {
            let total = mass_by_lot.entry(selection.lot()).or_default();
            *total = total
                .checked_add(u128::from(selection.mass().milligrams()))
                .ok_or(EatStackPortionsError::Eat(
                    EatError::InventoryMassOverflow { stockpile: source },
                ))?;
        }
    }
    let selections = mass_by_lot
        .into_iter()
        .map(|(lot, total)| {
            let mass = u64::try_from(total).map_err(|_| {
                EatStackPortionsError::Eat(EatError::InventoryMassOverflow { stockpile: source })
            })?;
            Ok(MaterialLotSelection::new(lot, Mass::from_milligrams(mass)))
        })
        .collect::<Result<Vec<_>, EatStackPortionsError>>()?;
    validate_eat(registries, state, source, &selections).map_err(EatStackPortionsError::Eat)
}

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
    let result = validate_eat_stack_to_metabolic_target(
        registries,
        state,
        stack,
        registries
            .survival()
            .physiology()
            .maximum_metabolic_energy(),
    );
    match result {
        Ok(Some(validated)) => {
            match selected_food_dietary_refill_mass(registries, state, stack.commodity()) {
                Some(mass) if mass > validated.planned_mass() => {
                    validate_eat_stack_up_to(registries, state, stack, mass)
                }
                _ => Ok(Some(validated)),
            }
        }
        Ok(None) => match selected_food_dietary_refill_mass(registries, state, stack.commodity()) {
            Some(mass) => validate_eat_stack_up_to(registries, state, stack, mass),
            None => Ok(None),
        },
        Err(EatStackToTargetError::Projection(
            MealMetabolicProjectionError::TargetUnreachableWithinIntakeLimit { maximum_meal_mass },
        )) => validate_eat_stack_up_to(registries, state, stack, maximum_meal_mass),
        Err(EatStackToTargetError::InsufficientStackMass { available, .. }) => {
            validate_eat_stack_up_to(registries, state, stack, available)
        }
        result => result,
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

fn validate_eat_stack_up_to(
    registries: &Registries,
    state: &AppState,
    stack: &MaterialStackProjection,
    desired: Mass,
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
    validate_eat(registries, state, source, &selections)
        .map(Some)
        .map_err(EatStackToTargetError::Eat)
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

/// Validates the minimum meal from one projected stack needed to reach `target`.
pub fn validate_eat_stack_to_metabolic_target(
    registries: &Registries,
    state: &AppState,
    stack: &MaterialStackProjection,
    target: Energy,
) -> Result<Option<ValidatedEat>, EatStackToTargetError> {
    let player = state
        .survival()
        .player()
        .copied()
        .ok_or(EatStackToTargetError::SurvivalNotInitialized)?;
    let source = stack.stockpile();
    validate_player_stockpile_access(state, source)
        .map_err(|error| EatStackToTargetError::Eat(EatError::Access(error)))?;
    let food = registries
        .survival()
        .get_food(stack.commodity())
        .copied()
        .ok_or(EatStackToTargetError::NotEdible {
            commodity: stack.commodity(),
        })?;
    let projected = project_minimum_meal_to_metabolic_target(
        registries.survival().physiology(),
        food,
        player.metabolic_energy(),
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
    validate_eat(registries, state, source, &selections)
        .map(Some)
        .map_err(EatStackToTargetError::Eat)
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

/// Familiar default drink-use action: drink a legal portion toward full hydration.
///
/// As with default food use, a selected vessel remains useful when it cannot fill the entire
/// reserve in one action. The exact target-oriented adapter keeps its stricter reachability
/// contract.
pub fn validate_drink_store_to_full(
    registries: &Registries,
    state: &AppState,
    store: FluidStoreId,
) -> Result<Option<ValidatedDrink>, DrinkStoreToTargetError> {
    let result = validate_drink_store_to_hydration_target(
        registries,
        state,
        store,
        registries.survival().physiology().maximum_hydration(),
    );
    match result {
        Err(DrinkStoreToTargetError::Projection(
            DrinkHydrationProjectionError::TargetUnreachableWithinIntakeLimit {
                maximum_drink_volume,
            },
        )) => validate_drink_store_up_to(registries, state, store, maximum_drink_volume),
        Err(DrinkStoreToTargetError::InsufficientVolume { available, .. }) => {
            validate_drink_store_up_to(registries, state, store, available)
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
    if portion < direct.minimum_meal_mass() {
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

fn validate_drink_store_up_to(
    registries: &Registries,
    state: &AppState,
    store: FluidStoreId,
    desired: Volume,
) -> Result<Option<ValidatedDrink>, DrinkStoreToTargetError> {
    validate_player_fluid_store_access(state, store)
        .map_err(|error| DrinkStoreToTargetError::Drink(DrinkError::Access(error)))?;
    let store_record = state
        .fluid()
        .get_store(store)
        .ok_or(DrinkStoreToTargetError::UnknownStore { store })?;
    let contents = store_record
        .contents()
        .ok_or(DrinkStoreToTargetError::EmptyStore { store })?;
    let direct = registries.survival().physiology().direct_consumption();
    let available = contents.volume();
    let portion = desired.min(available).min(direct.maximum_drink_volume());
    if portion < direct.minimum_drink_volume() {
        return Err(DrinkStoreToTargetError::InsufficientVolume {
            store,
            available,
            required: direct.minimum_drink_volume(),
        });
    }
    validate_drink(registries, state, store, portion)
        .map(Some)
        .map_err(DrinkStoreToTargetError::Drink)
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
    validate_player_fluid_store_access(state, store)
        .map_err(|error| DrinkStoreToTargetError::Drink(DrinkError::Access(error)))?;
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
