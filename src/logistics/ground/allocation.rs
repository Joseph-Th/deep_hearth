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
    StandingTreeOccupied { position: VoxelCoord },
    StockpileIdExhausted,
    InventoryRevisionExhausted,
    LogisticsRevisionExhausted,
}

/// Failure while allocating ordinary loose storage at the admitted player's current voxel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerGroundStockpileAllocationError {
    PlayerNotInitialized,
    PlayerDead,
    PlayerBusy,
    PreparedStrikeActive,
    Allocation(GroundStockpileAllocationError),
}

impl Display for PlayerGroundStockpileAllocationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlayerNotInitialized => {
                formatter.write_str("player logistics is not initialized")
            }
            Self::PlayerDead => formatter.write_str("dead player cannot allocate ground storage"),
            Self::PlayerBusy => {
                formatter.write_str("player cannot allocate ground storage while work is active")
            }
            Self::PreparedStrikeActive => formatter.write_str(
                "player must lower the prepared weapon before allocating ground storage",
            ),
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
            Self::PlayerNotInitialized
            | Self::PlayerDead
            | Self::PlayerBusy
            | Self::PreparedStrikeActive => None,
        }
    }
}

impl Display for GroundStockpileAllocationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroCapacity => formatter.write_str("ground stockpile capacity must be nonzero"),
            Self::StandingTreeOccupied { position } => write!(
                formatter,
                "ground stockpile cannot be allocated inside standing tree trunk at ({},{},{})",
                position.x(),
                position.y(),
                position.z()
            ),
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
    StalePlayerWorkRevision { expected: u64, actual: u64 },
    StaleCombatRevision { expected: u64, actual: u64 },
    StaleLogisticsRevision { expected: u64, actual: u64 },
    StaleInventoryRevision { expected: u64, actual: u64 },
    StandingTreeOccupied { position: VoxelCoord },
}

impl Display for GroundStockpileAllocationCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleSurvivalRevision { expected, actual } => write!(
                formatter,
                "ground stockpile allocation expected survival revision {expected} but current revision is {actual}"
            ),
            Self::StalePlayerWorkRevision { expected, actual } => write!(
                formatter,
                "ground stockpile allocation expected player work revision {expected} but current revision is {actual}"
            ),
            Self::StaleCombatRevision { expected, actual } => write!(
                formatter,
                "ground stockpile allocation expected combat revision {expected} but current revision is {actual}"
            ),
            Self::StaleLogisticsRevision { expected, actual } => write!(
                formatter,
                "ground stockpile allocation expected logistics revision {expected} but current revision is {actual}"
            ),
            Self::StaleInventoryRevision { expected, actual } => write!(
                formatter,
                "ground stockpile allocation expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::StandingTreeOccupied { position } => write!(
                formatter,
                "ground allocation cannot commit inside standing tree trunk at ({},{},{})",
                position.x(),
                position.y(),
                position.z()
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
    player_attention_revisions: Option<(u64, u64)>,
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
        if let Some((expected_work, expected_combat)) = self.player_attention_revisions {
            let actual_work = state.player_work().revision();
            if actual_work != expected_work {
                return Err(
                    GroundStockpileAllocationCommitError::StalePlayerWorkRevision {
                        expected: expected_work,
                        actual: actual_work,
                    },
                );
            }
            let actual_combat = state.combat().revision();
            if actual_combat != expected_combat {
                return Err(GroundStockpileAllocationCommitError::StaleCombatRevision {
                    expected: expected_combat,
                    actual: actual_combat,
                });
            }
        }
        // A new generated tree can occupy the requested voxel without changing
        // either inventory or logistics revisions. Recheck before allocating
        // custody so rejection does not leave a half-created stockpile.
        if state.world().tree_trunk_at(self.position).is_some() {
            return Err(GroundStockpileAllocationCommitError::StandingTreeOccupied {
                position: self.position,
            });
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
    // An empty location marker has no solid-terrain footprint, but any
    // stationary location in a standing trunk violates the canonical
    // world/tree occupancy invariant.
    if state.world().tree_trunk_at(position).is_some() {
        return Err(GroundStockpileAllocationError::StandingTreeOccupied { position });
    }
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
        player_attention_revisions: None,
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
    // Creating a stationary cache is a direct actor action, not an incidental
    // background transfer: timed work and raised melee preparation own attention.
    if state.player_work().active().is_some() {
        return Err(PlayerGroundStockpileAllocationError::PlayerBusy);
    }
    if state.combat().has_prepared_strike() {
        return Err(PlayerGroundStockpileAllocationError::PreparedStrikeActive);
    }
    let mut validated = validate_allocate_ground_stockpile(state, position, capacity)
        .map_err(PlayerGroundStockpileAllocationError::Allocation)?;
    validated.player_action = Some(player_action);
    validated.player_attention_revisions =
        Some((state.player_work().revision(), state.combat().revision()));
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

