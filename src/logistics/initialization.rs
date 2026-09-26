//! Player logistics initialization and finite carried-custody allocation.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::inventory::allocation::{
    EmptyStockpileAllocationCommitError, EmptyStockpileAllocationError,
    ValidatedEmptyStockpileAllocation, validate_empty_stockpile_allocation,
};
use crate::inventory::{StockpileId, StockpileStorageProfile};
use crate::spatial::VoxelCoord;

use super::PlayerLogisticsRecord;

/// Failure while validating initial player world/carrying custody.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitializePlayerLogisticsError {
    AlreadyInitialized,
    ZeroCarriedCapacity,
    StockpileIdExhausted,
    InventoryRevisionExhausted,
    LogisticsRevisionExhausted,
}

impl Display for InitializePlayerLogisticsError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyInitialized => {
                formatter.write_str("player logistics is already initialized")
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

impl Error for InitializePlayerLogisticsError {}

/// Failure when initial logistics authorization is committed against changed owner state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitializePlayerLogisticsCommitError {
    StaleLogisticsRevision { expected: u64, actual: u64 },
    StaleInventoryRevision { expected: u64, actual: u64 },
}

impl Display for InitializePlayerLogisticsCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleLogisticsRevision { expected, actual } => write!(
                formatter,
                "validated player logistics initialization expected logistics revision {expected} but current revision is {actual}"
            ),
            Self::StaleInventoryRevision { expected, actual } => write!(
                formatter,
                "validated player logistics initialization expected inventory revision {expected} but current revision is {actual}"
            ),
        }
    }
}

impl Error for InitializePlayerLogisticsCommitError {}

/// Revision-bound initial player position plus allocation of the inventory-owned carried stockpile.
#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub struct ValidatedPlayerLogisticsInitialization {
    expected_logistics_revision: u64,
    next_logistics_revision: u64,
    position: VoxelCoord,
    carried: ValidatedEmptyStockpileAllocation,
}

impl ValidatedPlayerLogisticsInitialization {
    #[must_use]
    pub const fn carried_stockpile(&self) -> StockpileId {
        self.carried.stockpile()
    }

    pub fn commit(
        self,
        state: &mut AppState,
    ) -> Result<PlayerLogisticsRecord, InitializePlayerLogisticsCommitError> {
        let actual_logistics_revision = state.logistics().revision();
        if actual_logistics_revision != self.expected_logistics_revision {
            return Err(
                InitializePlayerLogisticsCommitError::StaleLogisticsRevision {
                    expected: self.expected_logistics_revision,
                    actual: actual_logistics_revision,
                },
            );
        }
        let carried_stockpile = self.carried.commit(state).map_err(|error| match error {
            EmptyStockpileAllocationCommitError::StaleInventoryRevision { expected, actual } => {
                InitializePlayerLogisticsCommitError::StaleInventoryRevision { expected, actual }
            }
        })?;
        let player = PlayerLogisticsRecord::new(self.position, carried_stockpile);
        state.logistics_state_mut().apply_initialization(
            self.expected_logistics_revision,
            self.next_logistics_revision,
            player,
        );
        Ok(player)
    }
}

/// Validates the initial world position and finite solid-matter custody carried by the player.
///
/// The capacity is a bootstrap/world-authoring parameter, not an ordinary runtime mutation. This
/// operation intentionally creates no movement authority: later pickup, drop, haulage, and movement
/// actions must prove world access/path cost before changing custody or position.
pub fn validate_initialize_player_logistics(
    state: &AppState,
    position: VoxelCoord,
    carried_capacity: Mass,
) -> Result<ValidatedPlayerLogisticsInitialization, InitializePlayerLogisticsError> {
    if state.logistics().player().is_some() {
        return Err(InitializePlayerLogisticsError::AlreadyInitialized);
    }
    let carried = validate_empty_stockpile_allocation(
        state.inventory(),
        carried_capacity,
        StockpileStorageProfile::unbounded_solid_only(),
    )
    .map_err(|error| match error {
        EmptyStockpileAllocationError::ZeroCapacity => {
            InitializePlayerLogisticsError::ZeroCarriedCapacity
        }
        EmptyStockpileAllocationError::IdExhausted => {
            InitializePlayerLogisticsError::StockpileIdExhausted
        }
        EmptyStockpileAllocationError::RevisionExhausted => {
            InitializePlayerLogisticsError::InventoryRevisionExhausted
        }
    })?;
    let expected_logistics_revision = state.logistics().revision();
    let next_logistics_revision = expected_logistics_revision
        .checked_add(1)
        .ok_or(InitializePlayerLogisticsError::LogisticsRevisionExhausted)?;
    Ok(ValidatedPlayerLogisticsInitialization {
        expected_logistics_revision,
        next_logistics_revision,
        position,
        carried,
    })
}
