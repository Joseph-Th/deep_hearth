//! Atomic ordinary-player admission and respawn across survival, logistics, and carried custody.

mod authorization;
mod errors;

pub(crate) use authorization::{
    PlayerActionGuard, PlayerActionValidationError, StalePlayerSurvival,
    validate_optional_player_action, validate_player_action,
};
pub use errors::{
    PlayerAdmissionCommitError, PlayerAdmissionError, PlayerRespawnCommitError, PlayerRespawnError,
};

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
    ValidatedPlayerSurvivalInitialization, ValidatedPlayerSurvivalRespawn,
    validate_player_survival_initialization, validate_player_survival_respawn,
};

#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerRespawnOutcome {
    position: VoxelCoord,
    used_spawn_anchor: bool,
    death_position: VoxelCoord,
    death_cache_position: VoxelCoord,
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

    /// Actual resting position of the old carried stockpile and dropped tools.
    #[must_use]
    pub const fn death_cache_position(&self) -> VoxelCoord {
        self.death_cache_position
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
                PlayerLogisticsRespawnCommitError::SimulationTick { expected, actual } => {
                    PlayerRespawnCommitError::StaleSimulationTick { expected, actual }
                }
                PlayerLogisticsRespawnCommitError::LogisticsRevision { expected, actual } => {
                    PlayerRespawnCommitError::StaleLogisticsRevision { expected, actual }
                }
                PlayerLogisticsRespawnCommitError::InventoryRevision { expected, actual } => {
                    PlayerRespawnCommitError::StaleInventoryRevision { expected, actual }
                }
                PlayerLogisticsRespawnCommitError::WorldRevision { expected, actual } => {
                    PlayerRespawnCommitError::StaleWorldRevision { expected, actual }
                }
                PlayerLogisticsRespawnCommitError::StructureRevision { expected, actual } => {
                    PlayerRespawnCommitError::StaleStructureRevision { expected, actual }
                }
            })?;
        self.survival.apply(state);
        let logistics = self.logistics.apply(state);
        Ok(PlayerRespawnOutcome {
            position: logistics.position,
            used_spawn_anchor: logistics.used_spawn_anchor,
            death_position: logistics.death_position,
            death_cache_position: logistics.death_cache_position,
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
        PlayerLogisticsRespawnError::NoSafeRespawnLocation => {
            PlayerRespawnError::NoSafeRespawnLocation
        }
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
        InitializePlayerLogisticsError::BodyIntersectsTempleCaste { voxel } => {
            PlayerAdmissionError::BodyIntersectsTempleCaste { voxel }
        }
        InitializePlayerLogisticsError::BodyIntersectsStructure { voxel } => {
            PlayerAdmissionError::BodyIntersectsStructure { voxel }
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
/// carried equipment lands with it on the first solid floor beneath a generated-world death.
/// When no floor exists in that column, recovery places the cache at the safe respawn site.
/// The original death position is retained separately. The player receives a fresh empty
/// body-sized carried stockpile and returns to the original admitted spawn when it remains clear
/// and, in generated terrain, supported by the same floor geometry as continuous body motion.
/// An obstructed or unsupported generated anchor falls back to the death-body location when
/// that location is grounded. For an airborne generated-world death, a supported standing cell
/// directly above the anchor takes priority over neighboring collision-free, grounded positions
/// around the original anchor and then the death cache; the lower anchor column is an additional
/// fallback before rejecting the respawn if no safe location exists. Ungenerated authored
/// opening positions have no terrain floor authority; a clear death body remains a valid fallback.
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
        InitializePlayerLogisticsCommitError::SimulationTick { expected, actual } => {
            PlayerAdmissionCommitError::StaleSimulationTick { expected, actual }
        }
        InitializePlayerLogisticsCommitError::Logistics { expected, actual } => {
            PlayerAdmissionCommitError::StaleLogisticsRevision { expected, actual }
        }
        InitializePlayerLogisticsCommitError::Inventory { expected, actual } => {
            PlayerAdmissionCommitError::StaleInventoryRevision { expected, actual }
        }
        InitializePlayerLogisticsCommitError::World { expected, actual } => {
            PlayerAdmissionCommitError::StaleWorldRevision { expected, actual }
        }
        InitializePlayerLogisticsCommitError::Structure { expected, actual } => {
            PlayerAdmissionCommitError::StaleStructureRevision { expected, actual }
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
