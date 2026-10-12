//! Player admission and authoritative survival tick lifecycle.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Energy, Volume};
use crate::core::state::AppState;
use crate::registry::Registries;

use super::state::player_record;
use super::{NutritionReserves, PlayerSurvivalRecord, Vitality};

mod tick;

pub(crate) use tick::{SurvivalTickError, apply_survival_tick, decide_survival_tick};

/// Failure while admitting the local player into survival simulation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InitializeSurvivalError {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ValidatedPlayerSurvivalInitialization {
    expected_revision: u64,
    next_revision: u64,
    player: PlayerSurvivalRecord,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PlayerSurvivalInitializationCommitError {
    pub(crate) expected: u64,
    pub(crate) actual: u64,
}

impl ValidatedPlayerSurvivalInitialization {
    pub(crate) fn precheck(
        &self,
        state: &AppState,
    ) -> Result<(), PlayerSurvivalInitializationCommitError> {
        let actual = state.survival().revision();
        if actual != self.expected_revision {
            return Err(PlayerSurvivalInitializationCommitError {
                expected: self.expected_revision,
                actual,
            });
        }
        Ok(())
    }

    pub(crate) fn apply(self, state: &mut AppState) {
        state.survival_state_mut().apply_player(
            self.expected_revision,
            self.next_revision,
            self.player,
        );
    }
}

pub(crate) fn validate_player_survival_initialization_with_reserves(
    state: &AppState,
    metabolic_energy: Energy,
    hydration: Volume,
) -> Result<ValidatedPlayerSurvivalInitialization, InitializeSurvivalError> {
    if state.survival().player().is_some() {
        return Err(InitializeSurvivalError::AlreadyInitialized);
    }
    let expected_revision = state.survival().revision();
    let next_revision = expected_revision
        .checked_add(1)
        .ok_or(InitializeSurvivalError::RevisionExhausted)?;
    Ok(ValidatedPlayerSurvivalInitialization {
        expected_revision,
        next_revision,
        player: player_record(
            metabolic_energy,
            hydration,
            Vitality::MAXIMUM,
            NutritionReserves::FULL,
            0,
        ),
    })
}

pub(crate) fn validate_player_survival_initialization(
    registries: &Registries,
    state: &AppState,
) -> Result<ValidatedPlayerSurvivalInitialization, InitializeSurvivalError> {
    let physiology = registries.survival().physiology();
    validate_player_survival_initialization_with_reserves(
        state,
        physiology.maximum_metabolic_energy(),
        physiology.maximum_hydration(),
    )
}

/// Starts a capability-only gameplay fixture at authored full reserves.
///
/// Ordinary runtime admission must use the cross-owner player admission boundary so survival,
/// world position, and carried custody become authoritative atomically.
#[cfg(any(test, feature = "test-gameplay"))]
pub(crate) fn initialize_player_survival(
    registries: &Registries,
    state: &mut AppState,
) -> Result<(), InitializeSurvivalError> {
    validate_player_survival_initialization(registries, state)
        .map(|validated| validated.apply(state))
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
    validate_player_survival_initialization_with_reserves(
        state,
        physiology.maximum_metabolic_energy(),
        physiology.thirsty_below(),
    )
    .map(|validated| validated.apply(state))
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
    validate_player_survival_initialization_with_reserves(
        state,
        physiology.hungry_below(),
        physiology.maximum_hydration(),
    )
    .map(|validated| validated.apply(state))
}

#[cfg(test)]
#[path = "lifecycle_tests.rs"]
mod tests;
