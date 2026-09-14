//! Player admission and authoritative survival tick lifecycle.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Energy, Volume};
use crate::core::state::AppState;
use crate::registry::Registries;

use super::state::player_record;
use super::{NutritionReserves, Vitality};

mod tick;

pub(crate) use tick::{SurvivalTickError, apply_survival_tick, decide_survival_tick};

/// Failure while admitting the local player into survival simulation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitializeSurvivalError {
    AlreadyInitialized,
    RevisionExhausted,
}

impl Display for InitializeSurvivalError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyInitialized => {
                formatter.write_str("player survival is already initialized")
            }
            Self::RevisionExhausted => formatter.write_str("survival revision space is exhausted"),
        }
    }
}

impl Error for InitializeSurvivalError {}

fn initialize_player_survival_with_reserves(
    state: &mut AppState,
    metabolic_energy: Energy,
    hydration: Volume,
) -> Result<(), InitializeSurvivalError> {
    if state.survival().player().is_some() {
        return Err(InitializeSurvivalError::AlreadyInitialized);
    }
    let expected_revision = state.survival().revision();
    let next_revision = expected_revision
        .checked_add(1)
        .ok_or(InitializeSurvivalError::RevisionExhausted)?;
    state.survival_state_mut().apply_player(
        expected_revision,
        next_revision,
        player_record(
            metabolic_energy,
            hydration,
            Vitality::MAXIMUM,
            NutritionReserves::FULL,
            0,
        ),
    );
    Ok(())
}

/// Starts the local player's survival state at authored full reserves.
pub fn initialize_player_survival(
    registries: &Registries,
    state: &mut AppState,
) -> Result<(), InitializeSurvivalError> {
    let physiology = registries.survival().physiology();
    initialize_player_survival_with_reserves(
        state,
        physiology.maximum_metabolic_energy(),
        physiology.maximum_hydration(),
    )
}

/// Initializes the gameplay harness player exactly at the authored hydration warning boundary.
///
/// This is fixture-only starting-state construction. Runtime depletion still occurs exclusively
/// through authoritative simulation ticks after scenario setup returns.
#[cfg(feature = "test-gameplay")]
pub(crate) fn initialize_player_survival_at_hydration_warning_boundary_for_fixture(
    registries: &Registries,
    state: &mut AppState,
) -> Result<(), InitializeSurvivalError> {
    let physiology = registries.survival().physiology();
    initialize_player_survival_with_reserves(
        state,
        physiology.maximum_metabolic_energy(),
        physiology.thirsty_below(),
    )
}

/// Initializes the gameplay harness player exactly at the authored hunger warning boundary.
///
/// This is fixture-only starting-state construction. Runtime depletion still occurs exclusively
/// through authoritative simulation ticks after scenario setup returns.
#[cfg(feature = "test-gameplay")]
pub(crate) fn initialize_player_survival_at_hunger_warning_boundary_for_fixture(
    registries: &Registries,
    state: &mut AppState,
) -> Result<(), InitializeSurvivalError> {
    let physiology = registries.survival().physiology();
    initialize_player_survival_with_reserves(
        state,
        physiology.hungry_below(),
        physiology.maximum_hydration(),
    )
}

#[cfg(test)]
#[path = "lifecycle_tests.rs"]
mod tests;
