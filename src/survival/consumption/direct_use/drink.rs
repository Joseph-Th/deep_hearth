//! Selected-vessel drinking adapters over exact, finite fluid transactions.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Volume;
use crate::core::state::AppState;
use crate::fluid::FluidStoreId;
use crate::logistics::validate_player_fluid_store_access;
use crate::registry::Registries;
use crate::survival::Vitality;

use super::super::drinking::validate_drink_with_mode;
use super::super::{
    DrinkError, DrinkHydrationProjectionError, ValidatedDrink,
    project_minimum_drink_to_hydration_target,
};

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
    validate_drink_store_to_full_with_mode(registries, state, store, false)
}

/// Admit selected local drinking on the input control frame. The existing
/// validated transaction still owns fluid custody and scheduled absorption.
pub(crate) fn validate_drink_store_to_full_control_rate(
    registries: &Registries,
    state: &AppState,
    store: FluidStoreId,
) -> Result<Option<ValidatedDrink>, DrinkStoreToTargetError> {
    validate_drink_store_to_full_with_mode(registries, state, store, true)
}

/// A water source competes with gathering only if the full-use gesture can
/// actually be admitted. Preview tokens never escape this read-only check.
pub(crate) fn assess_drink_store_to_full_admission(
    registries: &Registries,
    state: &AppState,
    store: FluidStoreId,
) -> bool {
    matches!(
        validate_drink_store_to_full_with_mode(registries, state, store, true),
        Ok(Some(_))
    )
}

fn validate_drink_store_to_full_with_mode(
    registries: &Registries,
    state: &AppState,
    store: FluidStoreId,
    preview_off_boundary: bool,
) -> Result<Option<ValidatedDrink>, DrinkStoreToTargetError> {
    let result = validate_drink_store_to_hydration_target_with_mode(
        registries,
        state,
        store,
        registries.survival().physiology().maximum_hydration(),
        preview_off_boundary,
    );
    match result {
        Err(DrinkStoreToTargetError::Projection(
            DrinkHydrationProjectionError::TargetUnreachableWithinIntakeLimit {
                maximum_drink_volume,
            },
        )) => validate_drink_store_up_to(
            registries,
            state,
            store,
            maximum_drink_volume,
            preview_off_boundary,
        ),
        Err(DrinkStoreToTargetError::InsufficientVolume { available, .. }) => {
            validate_drink_store_up_to(registries, state, store, available, preview_off_boundary)
        }
        result => result,
    }
}

fn validate_drink_store_up_to(
    registries: &Registries,
    state: &AppState,
    store: FluidStoreId,
    desired: Volume,
    preview_off_boundary: bool,
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
    validate_drink_with_mode(registries, state, store, portion, preview_off_boundary)
        .map(Some)
        .map_err(DrinkStoreToTargetError::Drink)
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
    validate_drink_store_to_hydration_target_with_mode(registries, state, store, target, false)
}

fn validate_drink_store_to_hydration_target_with_mode(
    registries: &Registries,
    state: &AppState,
    store: FluidStoreId,
    target: Volume,
    preview_off_boundary: bool,
) -> Result<Option<ValidatedDrink>, DrinkStoreToTargetError> {
    let player = state
        .survival()
        .player()
        .copied()
        .ok_or(DrinkStoreToTargetError::SurvivalNotInitialized)?;
    // A full hydration reserve does not authorize a dead actor to use a
    // vessel; check before any source lookup or successful no-op projection.
    if player.vitality() == Vitality::ZERO {
        return Err(DrinkStoreToTargetError::Drink(DrinkError::PlayerDead));
    }
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
    validate_drink_with_mode(registries, state, store, required, preview_off_boundary)
        .map(Some)
        .map_err(DrinkStoreToTargetError::Drink)
}
