//! Ground stockpile custody allocation.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::inventory::allocation::{
    EmptyStockpileAllocationCommitError, EmptyStockpileAllocationError,
    ValidatedEmptyStockpileAllocation, validate_empty_stockpile_allocation,
};
use crate::inventory::{StockpileId, StockpileStorageProfile};
use crate::player::{PlayerActionGuard, validate_optional_player_action};
use crate::spatial::VoxelCoord;

/// Failure while allocating one empty loose ground stockpile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroundStockpileAllocationError {
    ZeroCapacity,
    StockpileIdExhausted,
    InventoryRevisionExhausted,
    LogisticsRevisionExhausted,
}

/// Failure while allocating ordinary loose storage at the admitted player's current voxel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerGroundStockpileAllocationError {
    PlayerNotInitialized,
    PlayerDead,
    Allocation(GroundStockpileAllocationError),
}

impl Display for PlayerGroundStockpileAllocationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlayerNotInitialized => {
                formatter.write_str("player logistics is not initialized")
            }
            Self::PlayerDead => formatter.write_str("dead player cannot allocate ground storage"),
            Self::Allocation(error) => {
                write!(formatter, "local ground storage allocation failed: {error}")
            }
        }
    }
}

impl Error for PlayerGroundStockpileAllocationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Allocation(error) => Some(error),
            Self::PlayerNotInitialized | Self::PlayerDead => None,
        }
    }
}

impl Display for GroundStockpileAllocationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroCapacity => formatter.write_str("ground stockpile capacity must be nonzero"),
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

impl Error for GroundStockpileAllocationError {}

/// Failure when an empty-ground allocation is committed against changed owner state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroundStockpileAllocationCommitError {
    StaleSurvivalRevision { expected: u64, actual: u64 },
    StaleLogisticsRevision { expected: u64, actual: u64 },
    StaleInventoryRevision { expected: u64, actual: u64 },
}

impl Display for GroundStockpileAllocationCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleSurvivalRevision { expected, actual } => write!(
                formatter,
                "ground stockpile allocation expected survival revision {expected} but current revision is {actual}"
            ),
            Self::StaleLogisticsRevision { expected, actual } => write!(
                formatter,
                "ground stockpile allocation expected logistics revision {expected} but current revision is {actual}"
            ),
            Self::StaleInventoryRevision { expected, actual } => write!(
                formatter,
                "ground stockpile allocation expected inventory revision {expected} but current revision is {actual}"
            ),
        }
    }
}

impl Error for GroundStockpileAllocationCommitError {}

/// Revision-bound creation of one empty inventory stockpile with a logistics-owned ground voxel.
#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub struct ValidatedGroundStockpileAllocation {
    player_action: Option<PlayerActionGuard>,
    expected_logistics_revision: u64,
    next_logistics_revision: u64,
    position: VoxelCoord,
    allocation: ValidatedEmptyStockpileAllocation,
}

impl ValidatedGroundStockpileAllocation {
    #[must_use]
    pub const fn stockpile(&self) -> StockpileId {
        self.allocation.stockpile()
    }

    pub fn commit(
        self,
        state: &mut AppState,
    ) -> Result<StockpileId, GroundStockpileAllocationCommitError> {
        let actual_logistics_revision = state.logistics().revision();
        if actual_logistics_revision != self.expected_logistics_revision {
            return Err(
                GroundStockpileAllocationCommitError::StaleLogisticsRevision {
                    expected: self.expected_logistics_revision,
                    actual: actual_logistics_revision,
                },
            );
        }
        if let Some(player_action) = self.player_action {
            player_action.precheck(state).map_err(|stale| {
                GroundStockpileAllocationCommitError::StaleSurvivalRevision {
                    expected: stale.expected,
                    actual: stale.actual,
                }
            })?;
        }
        let stockpile = self.allocation.commit(state).map_err(|error| match error {
            EmptyStockpileAllocationCommitError::StaleInventoryRevision { expected, actual } => {
                GroundStockpileAllocationCommitError::StaleInventoryRevision { expected, actual }
            }
        })?;
        state.logistics_state_mut().apply_ground_placement(
            self.expected_logistics_revision,
            self.next_logistics_revision,
            stockpile,
            self.position,
        );
        Ok(stockpile)
    }
}

/// Crate-owned allocation of one empty loose solid-matter stockpile at an authorized world voxel.
///
/// This creates custody and location only; it creates no matter and makes no claim about terrain,
/// resource generation, or how material arrives there. The caller supplies a finite capacity from
/// its own world/placement contract, while inventory remains authoritative for that capacity.
pub(crate) fn validate_allocate_ground_stockpile(
    state: &AppState,
    position: VoxelCoord,
    capacity: Mass,
) -> Result<ValidatedGroundStockpileAllocation, GroundStockpileAllocationError> {
    let allocation = validate_empty_stockpile_allocation(
        state.inventory(),
        capacity,
        StockpileStorageProfile::unbounded_solid_only(),
    )
    .map_err(|error| match error {
        EmptyStockpileAllocationError::ZeroCapacity => GroundStockpileAllocationError::ZeroCapacity,
        EmptyStockpileAllocationError::IdExhausted => {
            GroundStockpileAllocationError::StockpileIdExhausted
        }
        EmptyStockpileAllocationError::RevisionExhausted => {
            GroundStockpileAllocationError::InventoryRevisionExhausted
        }
    })?;
    if !state.can_spend_inventory_revisions(1) {
        return Err(GroundStockpileAllocationError::InventoryRevisionExhausted);
    }
    let expected_logistics_revision = state.logistics().revision();
    if !state.can_spend_logistics_revisions(1) {
        return Err(GroundStockpileAllocationError::LogisticsRevisionExhausted);
    }
    let next_logistics_revision = expected_logistics_revision
        .checked_add(1)
        .unwrap_or_else(|| unreachable!("logistics headroom check includes ground allocation"));
    Ok(ValidatedGroundStockpileAllocation {
        player_action: None,
        expected_logistics_revision,
        next_logistics_revision,
        position,
        allocation,
    })
}

pub(crate) fn validate_allocate_player_ground_stockpile_at(
    state: &AppState,
    position: VoxelCoord,
    capacity: Mass,
) -> Result<ValidatedGroundStockpileAllocation, PlayerGroundStockpileAllocationError> {
    let player_action = validate_optional_player_action(state)
        .map_err(|_| PlayerGroundStockpileAllocationError::PlayerDead)?;
    if state.logistics().player().is_none() {
        return Err(PlayerGroundStockpileAllocationError::PlayerNotInitialized);
    }
    let mut validated = validate_allocate_ground_stockpile(state, position, capacity)
        .map_err(PlayerGroundStockpileAllocationError::Allocation)?;
    validated.player_action = Some(player_action);
    Ok(validated)
}

/// Allocates one empty loose stockpile at the admitted player's exact current voxel.
///
/// Ordinary player-facing placement should use this boundary rather than supplying a coordinate.
/// The lower-level allocator remains available to world/bootstrap adapters that separately own why
/// a custody record exists at a specific voxel.
pub fn validate_allocate_player_ground_stockpile(
    state: &AppState,
    capacity: Mass,
) -> Result<ValidatedGroundStockpileAllocation, PlayerGroundStockpileAllocationError> {
    let position = state
        .logistics()
        .player()
        .map(|player| player.position())
        .ok_or(PlayerGroundStockpileAllocationError::PlayerNotInitialized)?;
    validate_allocate_player_ground_stockpile_at(state, position, capacity)
}
