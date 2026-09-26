//! Exact same-voxel material transfer between carried and ground custody.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::state::AppState;
use crate::inventory::{
    ExplicitConsumptionSelectionError, MaterialLotSelection, MaterialRelocationCommitError,
    MaterialRelocationError, StockpileId, ValidatedMaterialRelocation,
    validate_explicit_consumption_selection, validate_material_relocation_from_selection,
};
use crate::registry::Registries;
use crate::spatial::VoxelCoord;

use super::super::PlayerLogisticsRecord;

/// Failure while authorizing material movement between carried custody and a ground stockpile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GroundMaterialTransferError {
    PlayerUninitialized,
    GroundStockpileNotLocated {
        stockpile: StockpileId,
    },
    GroundStockpileNotAtPlayer {
        stockpile: StockpileId,
        ground: VoxelCoord,
        player: VoxelCoord,
    },
    Selection(ExplicitConsumptionSelectionError),
    Inventory(MaterialRelocationError),
}

impl Display for GroundMaterialTransferError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlayerUninitialized => {
                formatter.write_str("player logistics must be initialized before ground transfer")
            }
            Self::GroundStockpileNotLocated { stockpile } => write!(
                formatter,
                "stockpile {} has no logistics-owned ground location",
                stockpile.value()
            ),
            Self::GroundStockpileNotAtPlayer {
                stockpile,
                ground,
                player,
            } => write!(
                formatter,
                "stockpile {} is at voxel ({},{},{}) but player is at ({},{},{})",
                stockpile.value(),
                ground.x(),
                ground.y(),
                ground.z(),
                player.x(),
                player.y(),
                player.z()
            ),
            Self::Selection(error) => {
                write!(formatter, "ground transfer selection failed: {error}")
            }
            Self::Inventory(error) => {
                write!(formatter, "ground transfer inventory move failed: {error}")
            }
        }
    }
}

impl Error for GroundMaterialTransferError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Selection(error) => Some(error),
            Self::Inventory(error) => Some(error),
            Self::PlayerUninitialized
            | Self::GroundStockpileNotLocated { .. }
            | Self::GroundStockpileNotAtPlayer { .. } => None,
        }
    }
}

/// Failure when a validated same-voxel ground transfer becomes stale before commit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GroundMaterialTransferCommitError {
    StaleLogisticsRevision { expected: u64, actual: u64 },
    Inventory(MaterialRelocationCommitError),
}

impl Display for GroundMaterialTransferCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleLogisticsRevision { expected, actual } => write!(
                formatter,
                "ground transfer expected logistics revision {expected} but current revision is {actual}"
            ),
            Self::Inventory(error) => write!(formatter, "ground transfer commit failed: {error}"),
        }
    }
}

impl Error for GroundMaterialTransferCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Inventory(error) => Some(error),
            Self::StaleLogisticsRevision { .. } => None,
        }
    }
}

/// Revision-bound exact material movement between the player and one co-located ground stockpile.
#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub struct ValidatedGroundMaterialTransfer {
    expected_logistics_revision: u64,
    relocation: ValidatedMaterialRelocation,
}

impl ValidatedGroundMaterialTransfer {
    pub fn commit(self, state: &mut AppState) -> Result<(), GroundMaterialTransferCommitError> {
        let actual = state.logistics().revision();
        if actual != self.expected_logistics_revision {
            return Err(GroundMaterialTransferCommitError::StaleLogisticsRevision {
                expected: self.expected_logistics_revision,
                actual,
            });
        }
        self.relocation
            .commit(state)
            .map_err(GroundMaterialTransferCommitError::Inventory)
    }
}

fn validate_ground_access(
    state: &AppState,
    ground_stockpile: StockpileId,
) -> Result<PlayerLogisticsRecord, GroundMaterialTransferError> {
    let player = state
        .logistics()
        .player()
        .copied()
        .ok_or(GroundMaterialTransferError::PlayerUninitialized)?;
    let ground = state
        .logistics()
        .stationary_stockpile_position(ground_stockpile)
        .ok_or(GroundMaterialTransferError::GroundStockpileNotLocated {
            stockpile: ground_stockpile,
        })?;
    if ground != player.position() {
        return Err(GroundMaterialTransferError::GroundStockpileNotAtPlayer {
            stockpile: ground_stockpile,
            ground,
            player: player.position(),
        });
    }
    Ok(player)
}

/// Validates exact stack pickup from a ground stockpile at the player's current voxel.
pub fn validate_pickup_from_ground(
    registries: &Registries,
    state: &AppState,
    source: StockpileId,
    selections: &[MaterialLotSelection],
) -> Result<ValidatedGroundMaterialTransfer, GroundMaterialTransferError> {
    let player = validate_ground_access(state, source)?;
    let selection = validate_explicit_consumption_selection(state.inventory(), source, selections)
        .map_err(GroundMaterialTransferError::Selection)?;
    let relocation = validate_material_relocation_from_selection(
        registries,
        state,
        player.carried_stockpile(),
        selection,
    )
    .map_err(GroundMaterialTransferError::Inventory)?;
    Ok(ValidatedGroundMaterialTransfer {
        expected_logistics_revision: state.logistics().revision(),
        relocation,
    })
}

/// Validates exact stack drop from carried custody into a ground stockpile at the player's voxel.
pub fn validate_drop_to_ground(
    registries: &Registries,
    state: &AppState,
    destination: StockpileId,
    selections: &[MaterialLotSelection],
) -> Result<ValidatedGroundMaterialTransfer, GroundMaterialTransferError> {
    let player = validate_ground_access(state, destination)?;
    let selection = validate_explicit_consumption_selection(
        state.inventory(),
        player.carried_stockpile(),
        selections,
    )
    .map_err(GroundMaterialTransferError::Selection)?;
    let relocation =
        validate_material_relocation_from_selection(registries, state, destination, selection)
            .map_err(GroundMaterialTransferError::Inventory)?;
    Ok(ValidatedGroundMaterialTransfer {
        expected_logistics_revision: state.logistics().revision(),
        relocation,
    })
}
