//! Atomic ordinary-player admission across survival, logistics, and carried inventory custody.

use std::error::Error;
use std::fmt::{Display, Formatter};

#[cfg(feature = "test-gameplay")]
use crate::core::quantity::{Energy, Volume};
use crate::core::state::AppState;
use crate::logistics::{
    InitializePlayerLogisticsCommitError, InitializePlayerLogisticsError, PlayerLogisticsRecord,
    ValidatedPlayerLogisticsInitialization, validate_player_logistics_initialization,
};
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
#[cfg(feature = "test-gameplay")]
use crate::survival::validate_player_survival_initialization_with_reserves;
use crate::survival::{
    InitializeSurvivalError, PlayerSurvivalInitializationCommitError,
    ValidatedPlayerSurvivalInitialization, validate_player_survival_initialization,
};

/// Failure while validating ordinary player admission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerAdmissionError {
    AlreadyAdmitted,
    PartialExistingAdmission {
        survival_initialized: bool,
        logistics_initialized: bool,
    },
    SurvivalRevisionExhausted,
    ZeroCarriedCapacity,
    StockpileIdExhausted,
    InventoryRevisionExhausted,
    LogisticsRevisionExhausted,
}

impl Display for PlayerAdmissionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyAdmitted => formatter.write_str("player is already admitted"),
            Self::PartialExistingAdmission {
                survival_initialized,
                logistics_initialized,
            } => write!(
                formatter,
                "player admission is inconsistent: survival initialized={survival_initialized}, logistics initialized={logistics_initialized}"
            ),
            Self::SurvivalRevisionExhausted => {
                formatter.write_str("survival revision space is exhausted")
            }
            Self::ZeroCarriedCapacity => {
                formatter.write_str("player carried inventory capacity must be nonzero")
            }
            Self::StockpileIdExhausted => {
                formatter.write_str("stockpile identifier space is exhausted")
            }
            Self::InventoryRevisionExhausted => {
                formatter.write_str("inventory revision space is exhausted")
            }
            Self::LogisticsRevisionExhausted => {
                formatter.write_str("logistics revision space is exhausted")
            }
        }
    }
}

impl Error for PlayerAdmissionError {}

/// Failure when validated player admission is committed after one of its owners changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerAdmissionCommitError {
    StaleSurvivalRevision { expected: u64, actual: u64 },
    StaleLogisticsRevision { expected: u64, actual: u64 },
    StaleInventoryRevision { expected: u64, actual: u64 },
}

impl Display for PlayerAdmissionCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleSurvivalRevision { expected, actual } => write!(
                formatter,
                "validated player admission expected survival revision {expected} but current revision is {actual}"
            ),
            Self::StaleLogisticsRevision { expected, actual } => write!(
                formatter,
                "validated player admission expected logistics revision {expected} but current revision is {actual}"
            ),
            Self::StaleInventoryRevision { expected, actual } => write!(
                formatter,
                "validated player admission expected inventory revision {expected} but current revision is {actual}"
            ),
        }
    }
}

impl Error for PlayerAdmissionCommitError {}

/// Revision-bound authorization to establish all ordinary player-owned runtime state together.
#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub struct ValidatedPlayerAdmission {
    survival: ValidatedPlayerSurvivalInitialization,
    logistics: ValidatedPlayerLogisticsInitialization,
}

impl ValidatedPlayerAdmission {
    /// Commits survival, world position, and carried custody only after every owner precheck passes.
    pub fn commit(
        self,
        state: &mut AppState,
    ) -> Result<PlayerLogisticsRecord, PlayerAdmissionCommitError> {
        self.survival.precheck(state).map_err(
            |PlayerSurvivalInitializationCommitError { expected, actual }| {
                PlayerAdmissionCommitError::StaleSurvivalRevision { expected, actual }
            },
        )?;
        self.logistics
            .precheck(state)
            .map_err(map_logistics_commit_error)?;

        self.survival.apply(state);
        Ok(self.logistics.apply(state))
    }
}

fn map_logistics_error(error: InitializePlayerLogisticsError) -> PlayerAdmissionError {
    match error {
        InitializePlayerLogisticsError::AlreadyInitialized => {
            unreachable!(
                "player admission checked existing owner state before logistics validation"
            )
        }
        InitializePlayerLogisticsError::ZeroCarriedCapacity => {
            PlayerAdmissionError::ZeroCarriedCapacity
        }
        InitializePlayerLogisticsError::StockpileIdExhausted => {
            PlayerAdmissionError::StockpileIdExhausted
        }
        InitializePlayerLogisticsError::InventoryRevisionExhausted => {
            PlayerAdmissionError::InventoryRevisionExhausted
        }
        InitializePlayerLogisticsError::LogisticsRevisionExhausted => {
            PlayerAdmissionError::LogisticsRevisionExhausted
        }
    }
}

fn map_survival_error(error: InitializeSurvivalError) -> PlayerAdmissionError {
    match error {
        InitializeSurvivalError::AlreadyInitialized => {
            unreachable!("player admission checked existing owner state before survival validation")
        }
        InitializeSurvivalError::RevisionExhausted => {
            PlayerAdmissionError::SurvivalRevisionExhausted
        }
    }
}

fn map_logistics_commit_error(
    error: InitializePlayerLogisticsCommitError,
) -> PlayerAdmissionCommitError {
    match error {
        InitializePlayerLogisticsCommitError::StaleLogisticsRevision { expected, actual } => {
            PlayerAdmissionCommitError::StaleLogisticsRevision { expected, actual }
        }
        InitializePlayerLogisticsCommitError::StaleInventoryRevision { expected, actual } => {
            PlayerAdmissionCommitError::StaleInventoryRevision { expected, actual }
        }
    }
}

fn validate_owner_absence(state: &AppState) -> Result<(), PlayerAdmissionError> {
    let survival_initialized = state.survival().player().is_some();
    let logistics_initialized = state.logistics().player().is_some();
    match (survival_initialized, logistics_initialized) {
        (false, false) => Ok(()),
        (true, true) => Err(PlayerAdmissionError::AlreadyAdmitted),
        _ => Err(PlayerAdmissionError::PartialExistingAdmission {
            survival_initialized,
            logistics_initialized,
        }),
    }
}

fn validate_admission(
    registries: &Registries,
    state: &AppState,
    position: VoxelCoord,
    survival: ValidatedPlayerSurvivalInitialization,
) -> Result<ValidatedPlayerAdmission, PlayerAdmissionError> {
    let logistics = validate_player_logistics_initialization(registries, state, position)
        .map_err(map_logistics_error)?;
    Ok(ValidatedPlayerAdmission {
        survival,
        logistics,
    })
}

/// Validates one ordinary player admission at authored full survival reserves.
///
/// This is the canonical runtime boundary. Survival, player world position, and the inventory-owned
/// carried stockpile are authorized against the same pre-admission snapshot and commit atomically.
pub fn validate_admit_player(
    registries: &Registries,
    state: &AppState,
    position: VoxelCoord,
) -> Result<ValidatedPlayerAdmission, PlayerAdmissionError> {
    validate_owner_absence(state)?;
    let survival =
        validate_player_survival_initialization(registries, state).map_err(map_survival_error)?;
    validate_admission(registries, state, position, survival)
}

#[cfg(feature = "test-gameplay")]
pub(crate) fn validate_admit_player_with_reserves_for_fixture(
    registries: &Registries,
    state: &AppState,
    position: VoxelCoord,
    metabolic_energy: Energy,
    hydration: Volume,
) -> Result<ValidatedPlayerAdmission, PlayerAdmissionError> {
    validate_owner_absence(state)?;
    let survival =
        validate_player_survival_initialization_with_reserves(state, metabolic_energy, hydration)
            .map_err(map_survival_error)?;
    validate_admission(registries, state, position, survival)
}

#[cfg(test)]
#[path = "player_tests.rs"]
mod tests;
