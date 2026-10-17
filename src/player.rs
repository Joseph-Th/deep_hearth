//! Atomic ordinary-player admission and respawn across survival, logistics, and carried custody.

use std::error::Error;
use std::fmt::{Display, Formatter};

#[cfg(feature = "test-gameplay")]
use crate::core::quantity::{Energy, Volume};
use crate::core::state::AppState;
use crate::equipment::EquipmentId;
use crate::inventory::StockpileId;
use crate::logistics::{
    InitializePlayerLogisticsCommitError, InitializePlayerLogisticsError, PlayerLogisticsRecord,
    PlayerLogisticsRespawnCommitError, PlayerLogisticsRespawnError,
    ValidatedPlayerLogisticsInitialization, ValidatedPlayerLogisticsRespawn,
    validate_player_logistics_initialization, validate_player_logistics_respawn,
};
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
#[cfg(feature = "test-gameplay")]
use crate::survival::validate_player_survival_initialization_with_reserves;
use crate::survival::{
    InitializeSurvivalError, PlayerSurvivalInitializationCommitError, RespawnSurvivalError,
    ValidatedPlayerSurvivalInitialization, ValidatedPlayerSurvivalRespawn, Vitality,
    validate_player_survival_initialization, validate_player_survival_respawn,
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
    PositionOutsideContinuousWorld {
        position: VoxelCoord,
    },
    BodyIntersectsSolid {
        voxel: VoxelCoord,
    },
    ZeroCarriedCapacity,
    StockpileIdExhausted,
    InventoryRevisionExhausted,
    LogisticsRevisionExhausted,
}

/// Survival-snapshot authorization proof for direct player-owned manipulation.
///
/// Owner APIs also serve controlled capability setup before ordinary player admission. Once a
/// player is fully admitted, however, the same operation is player-owned and terminal vitality
/// must block it. The proof captures survival revision so an authorization granted to a living
/// player cannot commit after survival changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PlayerActionGuard {
    expected_survival_revision: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PlayerActionValidationError {
    PlayerNotAdmitted,
    PlayerDead,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PlayerDeadActionError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StalePlayerSurvival {
    pub(crate) expected: u64,
    pub(crate) actual: u64,
}

impl PlayerActionGuard {
    pub(crate) fn precheck(self, state: &AppState) -> Result<(), StalePlayerSurvival> {
        let Some(expected) = self.expected_survival_revision else {
            return Ok(());
        };
        let actual = state.survival().revision();
        if actual != expected {
            return Err(StalePlayerSurvival { expected, actual });
        }
        Ok(())
    }
}

pub(crate) fn validate_optional_player_action(
    state: &AppState,
) -> Result<PlayerActionGuard, PlayerDeadActionError> {
    if player_admission_state(state) != PlayerAdmissionState::Admitted {
        return Ok(PlayerActionGuard {
            expected_survival_revision: None,
        });
    }
    let player = state
        .survival()
        .player()
        .unwrap_or_else(|| unreachable!("admitted player has survival state"));
    if player.vitality() == Vitality::ZERO {
        return Err(PlayerDeadActionError);
    }
    Ok(PlayerActionGuard {
        expected_survival_revision: Some(state.survival().revision()),
    })
}

pub(crate) fn validate_player_action(
    state: &AppState,
) -> Result<PlayerActionGuard, PlayerActionValidationError> {
    if player_admission_state(state) != PlayerAdmissionState::Admitted {
        return Err(PlayerActionValidationError::PlayerNotAdmitted);
    }
    let player = state
        .survival()
        .player()
        .unwrap_or_else(|| unreachable!("admitted player has survival state"));
    if player.vitality() == Vitality::ZERO {
        return Err(PlayerActionValidationError::PlayerDead);
    }
    Ok(PlayerActionGuard {
        expected_survival_revision: Some(state.survival().revision()),
    })
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
            Self::PositionOutsideContinuousWorld { position } => write!(
                formatter,
                "player position ({},{},{}) cannot be represented by continuous body coordinates",
                position.x(),
                position.y(),
                position.z()
            ),
            Self::BodyIntersectsSolid { voxel } => write!(
                formatter,
                "player body intersects solid world voxel ({},{},{})",
                voxel.x(),
                voxel.y(),
                voxel.z()
            ),
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerRespawnError {
    PlayerNotAdmitted,
    PlayerAlive,
    ActivePlayerWork,
    PendingDirectAction,
    PendingDirectConsumption,
    SurvivalRevisionExhausted,
    StockpileIdExhausted,
    InventoryRevisionExhausted,
    LogisticsRevisionExhausted,
}

impl Display for PlayerRespawnError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlayerNotAdmitted => formatter.write_str("player is not admitted"),
            Self::PlayerAlive => formatter.write_str("living player cannot respawn"),
            Self::ActivePlayerWork => {
                formatter.write_str("player cannot respawn while work is active")
            }
            Self::PendingDirectAction => {
                formatter.write_str("player cannot respawn with buffered direct input")
            }
            Self::PendingDirectConsumption => {
                formatter.write_str("player cannot respawn with pending direct consumption")
            }
            Self::SurvivalRevisionExhausted => {
                formatter.write_str("survival revision space is exhausted")
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

impl Error for PlayerRespawnError {}

/// Failure when validated player admission is committed after one of its owners changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerAdmissionCommitError {
    StaleSurvivalRevision { expected: u64, actual: u64 },
    StaleLogisticsRevision { expected: u64, actual: u64 },
    StaleInventoryRevision { expected: u64, actual: u64 },
    StaleWorldRevision { expected: u64, actual: u64 },
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
            Self::StaleWorldRevision { expected, actual } => write!(
                formatter,
                "validated player admission expected world revision {expected} but current revision is {actual}"
            ),
        }
    }
}

impl Error for PlayerAdmissionCommitError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerRespawnCommitError {
    StaleSurvivalRevision { expected: u64, actual: u64 },
    StaleLogisticsRevision { expected: u64, actual: u64 },
    StaleInventoryRevision { expected: u64, actual: u64 },
    StaleWorldRevision { expected: u64, actual: u64 },
}

impl Display for PlayerRespawnCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleSurvivalRevision { expected, actual } => write!(
                formatter,
                "validated respawn expected survival revision {expected} but current revision is {actual}"
            ),
            Self::StaleLogisticsRevision { expected, actual } => write!(
                formatter,
                "validated respawn expected logistics revision {expected} but current revision is {actual}"
            ),
            Self::StaleInventoryRevision { expected, actual } => write!(
                formatter,
                "validated respawn expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::StaleWorldRevision { expected, actual } => write!(
                formatter,
                "validated respawn expected world revision {expected} but current revision is {actual}"
            ),
        }
    }
}

impl Error for PlayerRespawnCommitError {}

#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerRespawnOutcome {
    position: VoxelCoord,
    used_spawn_anchor: bool,
    death_position: VoxelCoord,
    death_cache: StockpileId,
    carried_stockpile: StockpileId,
    dropped_equipment: Vec<EquipmentId>,
}

impl PlayerRespawnOutcome {
    #[must_use]
    pub const fn position(&self) -> VoxelCoord {
        self.position
    }

    #[must_use]
    pub const fn used_spawn_anchor(&self) -> bool {
        self.used_spawn_anchor
    }

    #[must_use]
    pub const fn death_position(&self) -> VoxelCoord {
        self.death_position
    }

    #[must_use]
    pub const fn death_cache(&self) -> StockpileId {
        self.death_cache
    }

    #[must_use]
    pub const fn carried_stockpile(&self) -> StockpileId {
        self.carried_stockpile
    }

    pub fn dropped_equipment(&self) -> &[EquipmentId] {
        &self.dropped_equipment
    }
}

/// Revision-bound authorization to establish all ordinary player-owned runtime state together.
#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub struct ValidatedPlayerAdmission {
    survival: ValidatedPlayerSurvivalInitialization,
    logistics: ValidatedPlayerLogisticsInitialization,
}

#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub struct ValidatedPlayerRespawn {
    survival: ValidatedPlayerSurvivalRespawn,
    logistics: ValidatedPlayerLogisticsRespawn,
}

impl ValidatedPlayerRespawn {
    pub fn commit(
        self,
        state: &mut AppState,
    ) -> Result<PlayerRespawnOutcome, PlayerRespawnCommitError> {
        self.survival.precheck(state).map_err(
            |PlayerSurvivalInitializationCommitError { expected, actual }| {
                PlayerRespawnCommitError::StaleSurvivalRevision { expected, actual }
            },
        )?;
        self.logistics
            .precheck(state)
            .map_err(|error| match error {
                PlayerLogisticsRespawnCommitError::StaleLogisticsRevision { expected, actual } => {
                    PlayerRespawnCommitError::StaleLogisticsRevision { expected, actual }
                }
                PlayerLogisticsRespawnCommitError::StaleInventoryRevision { expected, actual } => {
                    PlayerRespawnCommitError::StaleInventoryRevision { expected, actual }
                }
                PlayerLogisticsRespawnCommitError::StaleWorldRevision { expected, actual } => {
                    PlayerRespawnCommitError::StaleWorldRevision { expected, actual }
                }
            })?;
        self.survival.apply(state);
        let logistics = self.logistics.apply(state);
        Ok(PlayerRespawnOutcome {
            position: logistics.position,
            used_spawn_anchor: logistics.used_spawn_anchor,
            death_position: logistics.death_position,
            death_cache: logistics.death_cache,
            carried_stockpile: logistics.carried_stockpile,
            dropped_equipment: logistics.dropped_equipment,
        })
    }
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

fn map_respawn_survival_error(error: RespawnSurvivalError) -> PlayerRespawnError {
    match error {
        RespawnSurvivalError::PlayerNotInitialized => PlayerRespawnError::PlayerNotAdmitted,
        RespawnSurvivalError::PlayerAlive => PlayerRespawnError::PlayerAlive,
        RespawnSurvivalError::PendingDirectConsumption => {
            PlayerRespawnError::PendingDirectConsumption
        }
        RespawnSurvivalError::RevisionExhausted => PlayerRespawnError::SurvivalRevisionExhausted,
    }
}

fn map_respawn_logistics_error(error: PlayerLogisticsRespawnError) -> PlayerRespawnError {
    match error {
        PlayerLogisticsRespawnError::PlayerNotInitialized => PlayerRespawnError::PlayerNotAdmitted,
        PlayerLogisticsRespawnError::ActivePlayerWork => PlayerRespawnError::ActivePlayerWork,
        PlayerLogisticsRespawnError::PendingDirectAction => PlayerRespawnError::PendingDirectAction,
        PlayerLogisticsRespawnError::StockpileIdExhausted => {
            PlayerRespawnError::StockpileIdExhausted
        }
        PlayerLogisticsRespawnError::InventoryRevisionExhausted => {
            PlayerRespawnError::InventoryRevisionExhausted
        }
        PlayerLogisticsRespawnError::LogisticsRevisionExhausted => {
            PlayerRespawnError::LogisticsRevisionExhausted
        }
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
        InitializePlayerLogisticsError::PositionOutsideContinuousWorld { position } => {
            PlayerAdmissionError::PositionOutsideContinuousWorld { position }
        }
        InitializePlayerLogisticsError::BodyIntersectsSolid { voxel } => {
            PlayerAdmissionError::BodyIntersectsSolid { voxel }
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

/// Validates explicit recovery from terminal player survival state.
///
/// Exact carried matter remains in the old inventory stockpile, which becomes a local death cache;
/// carried equipment becomes stationary at the death voxel. The player receives a fresh empty
/// body-sized carried stockpile and returns to the original admitted spawn when it remains clear.
/// If that anchor is obstructed, respawn falls back to the valid death-body location rather than
/// soft-locking the dead player.
pub fn validate_respawn_player(
    registries: &Registries,
    state: &AppState,
) -> Result<ValidatedPlayerRespawn, PlayerRespawnError> {
    if player_admission_state(state) != PlayerAdmissionState::Admitted {
        return Err(PlayerRespawnError::PlayerNotAdmitted);
    }
    let survival =
        validate_player_survival_respawn(registries, state).map_err(map_respawn_survival_error)?;
    let logistics = validate_player_logistics_respawn(registries, state)
        .map_err(map_respawn_logistics_error)?;
    Ok(ValidatedPlayerRespawn {
        survival,
        logistics,
    })
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
        InitializePlayerLogisticsCommitError::Logistics { expected, actual } => {
            PlayerAdmissionCommitError::StaleLogisticsRevision { expected, actual }
        }
        InitializePlayerLogisticsCommitError::Inventory { expected, actual } => {
            PlayerAdmissionCommitError::StaleInventoryRevision { expected, actual }
        }
        InitializePlayerLogisticsCommitError::World { expected, actual } => {
            PlayerAdmissionCommitError::StaleWorldRevision { expected, actual }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PlayerAdmissionState {
    Absent,
    Admitted,
    Partial {
        survival_initialized: bool,
        logistics_initialized: bool,
    },
}

#[must_use]
pub(crate) fn player_admission_state(state: &AppState) -> PlayerAdmissionState {
    let survival_initialized = state.survival().player().is_some();
    let logistics_initialized = state.logistics().player().is_some();
    match (survival_initialized, logistics_initialized) {
        (false, false) => PlayerAdmissionState::Absent,
        (true, true) => PlayerAdmissionState::Admitted,
        _ => PlayerAdmissionState::Partial {
            survival_initialized,
            logistics_initialized,
        },
    }
}

fn validate_owner_absence(state: &AppState) -> Result<(), PlayerAdmissionError> {
    match player_admission_state(state) {
        PlayerAdmissionState::Absent => Ok(()),
        PlayerAdmissionState::Admitted => Err(PlayerAdmissionError::AlreadyAdmitted),
        PlayerAdmissionState::Partial {
            survival_initialized,
            logistics_initialized,
        } => Err(PlayerAdmissionError::PartialExistingAdmission {
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
