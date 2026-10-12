//! Player logistics initialization and finite carried-custody allocation.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
#[cfg(test)]
use crate::inventory::StockpileId;
use crate::inventory::StockpileStorageProfile;
use crate::inventory::allocation::{
    EmptyStockpileAllocationCommitError, EmptyStockpileAllocationError,
    ValidatedEmptyStockpileAllocation, validate_empty_stockpile_allocation,
};
use crate::registry::Registries;
use crate::spatial::VoxelCoord;

use super::PlayerLogisticsRecord;

/// Failure while validating initial player world/carrying custody.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InitializePlayerLogisticsError {
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
pub(crate) enum InitializePlayerLogisticsCommitError {
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
pub(crate) struct ValidatedPlayerLogisticsInitialization {
    expected_logistics_revision: u64,
    next_logistics_revision: u64,
    position: VoxelCoord,
    carried: ValidatedEmptyStockpileAllocation,
}

impl ValidatedPlayerLogisticsInitialization {
    #[must_use]
    #[cfg(test)]
    pub(crate) const fn carried_stockpile(&self) -> StockpileId {
        self.carried.stockpile()
    }

    pub(crate) fn precheck(
        &self,
        state: &AppState,
    ) -> Result<(), InitializePlayerLogisticsCommitError> {
        let actual_logistics_revision = state.logistics().revision();
        if actual_logistics_revision != self.expected_logistics_revision {
            return Err(
                InitializePlayerLogisticsCommitError::StaleLogisticsRevision {
                    expected: self.expected_logistics_revision,
                    actual: actual_logistics_revision,
                },
            );
        }
        self.carried.precheck(state).map_err(|error| match error {
            EmptyStockpileAllocationCommitError::StaleInventoryRevision { expected, actual } => {
                InitializePlayerLogisticsCommitError::StaleInventoryRevision { expected, actual }
            }
        })
    }

    pub(crate) fn apply(self, state: &mut AppState) -> PlayerLogisticsRecord {
        let carried_stockpile = self.carried.apply(state);
        let player = PlayerLogisticsRecord::new(self.position, carried_stockpile);
        state.logistics_state_mut().apply_initialization(
            self.expected_logistics_revision,
            self.next_logistics_revision,
            player,
        );
        player
    }

    #[cfg(test)]
    pub(crate) fn commit(
        self,
        state: &mut AppState,
    ) -> Result<PlayerLogisticsRecord, InitializePlayerLogisticsCommitError> {
        self.precheck(state)?;
        Ok(self.apply(state))
    }
}

fn validate_initialization_with_capacity(
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
    if !state.can_spend_inventory_revisions(1) {
        return Err(InitializePlayerLogisticsError::InventoryRevisionExhausted);
    }
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

/// Prepares the logistics/inventory component of ordinary player admission.
///
/// The cross-owner `player` boundary owns ordinary admission. This owner-level helper contributes
/// only the initial world position and physiology-sized carried stockpile so callers cannot admit a
/// location/custody half-state as ordinary play. It intentionally creates no movement authority:
/// later pickup, drop, haulage, and movement actions must prove world access/path cost before
/// changing custody or position.
pub(crate) fn validate_player_logistics_initialization(
    registries: &Registries,
    state: &AppState,
    position: VoxelCoord,
) -> Result<ValidatedPlayerLogisticsInitialization, InitializePlayerLogisticsError> {
    validate_initialization_with_capacity(
        state,
        position,
        registries.survival().physiology().carried_mass_capacity(),
    )
}

/// Test-only owner initialization for contracts that intentionally exercise arbitrary capacity.
///
/// Gameplay probes should use the ordinary cross-owner player admission boundary unless the
/// capacity itself is the fixture variable under test.
#[cfg(test)]
pub(crate) fn validate_initialize_player_logistics(
    state: &AppState,
    position: VoxelCoord,
    carried_capacity: Mass,
) -> Result<ValidatedPlayerLogisticsInitialization, InitializePlayerLogisticsError> {
    validate_initialization_with_capacity(state, position, carried_capacity)
}
