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

use super::{PlayerBodyState, PlayerLogisticsRecord};

/// Failure while validating initial player world/carrying custody.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InitializePlayerLogisticsError {
    AlreadyInitialized,
    PositionOutsideContinuousWorld { position: VoxelCoord },
    BodyIntersectsSolid { voxel: VoxelCoord },
    BodyIntersectsTempleCaste { voxel: VoxelCoord },
    BodyIntersectsStructure { voxel: VoxelCoord },
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
            Self::BodyIntersectsTempleCaste { voxel } => write!(
                formatter,
                "player body intersects temple resident at ({},{},{})",
                voxel.x(),
                voxel.y(),
                voxel.z()
            ),
            Self::BodyIntersectsStructure { voxel } => write!(
                formatter,
                "player body intersects completed structural matter at ({},{},{})",
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

impl Error for InitializePlayerLogisticsError {}

/// Failure when initial logistics authorization is committed against changed owner state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InitializePlayerLogisticsCommitError {
    Logistics { expected: u64, actual: u64 },
    Inventory { expected: u64, actual: u64 },
    World { expected: u64, actual: u64 },
    Structure { expected: u64, actual: u64 },
}

impl Display for InitializePlayerLogisticsCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Logistics { expected, actual } => write!(
                formatter,
                "validated player logistics initialization expected logistics revision {expected} but current revision is {actual}"
            ),
            Self::Inventory { expected, actual } => write!(
                formatter,
                "validated player logistics initialization expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::World { expected, actual } => write!(
                formatter,
                "validated player logistics initialization expected world revision {expected} but current revision is {actual}"
            ),
            Self::Structure { expected, actual } => write!(
                formatter,
                "validated player logistics initialization expected structure revision {expected} but current revision is {actual}"
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
    expected_world_revision: u64,
    expected_structure_revision: u64,
    next_logistics_revision: u64,
    body: PlayerBodyState,
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
            return Err(InitializePlayerLogisticsCommitError::Logistics {
                expected: self.expected_logistics_revision,
                actual: actual_logistics_revision,
            });
        }
        let actual_world_revision = state.world().revision();
        if actual_world_revision != self.expected_world_revision {
            return Err(InitializePlayerLogisticsCommitError::World {
                expected: self.expected_world_revision,
                actual: actual_world_revision,
            });
        }
        let actual_structure_revision = state.structures().revision();
        if actual_structure_revision != self.expected_structure_revision {
            return Err(InitializePlayerLogisticsCommitError::Structure {
                expected: self.expected_structure_revision,
                actual: actual_structure_revision,
            });
        }
        self.carried.precheck(state).map_err(|error| match error {
            EmptyStockpileAllocationCommitError::StaleInventoryRevision { expected, actual } => {
                InitializePlayerLogisticsCommitError::Inventory { expected, actual }
            }
        })
    }

    pub(crate) fn apply(self, state: &mut AppState) -> PlayerLogisticsRecord {
        let carried_stockpile = self.carried.apply(state);
        let player = PlayerLogisticsRecord::new(self.body, carried_stockpile);
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
    let body = PlayerBodyState::centered_on_voxel_floor(position)
        .ok_or(InitializePlayerLogisticsError::PositionOutsideContinuousWorld { position })?;
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
        expected_world_revision: state.world().revision(),
        expected_structure_revision: state.structures().revision(),
        next_logistics_revision,
        body,
        carried,
    })
}

/// Prepares the logistics/inventory component of ordinary player admission.
///
/// The cross-owner `player` boundary owns ordinary admission. This owner-level helper contributes
/// only the initial world position and physiology-sized carried stockpile so callers cannot admit a
/// location/custody half-state as ordinary play. It intentionally creates no movement authority:
/// later custody changes must satisfy their owning access constraints, while continuous body motion
/// derives legality from generated-world collision and current work/custody state.
pub(crate) fn validate_player_logistics_initialization(
    registries: &Registries,
    state: &AppState,
    position: VoxelCoord,
) -> Result<ValidatedPlayerLogisticsInitialization, InitializePlayerLogisticsError> {
    let validated = validate_initialization_with_capacity(
        state,
        position,
        registries.survival().physiology().carried_mass_capacity(),
    )?;
    if !crate::logistics::player_body_fits_continuous_world(
        validated.body,
        registries.survival().physiology().locomotion(),
    ) {
        return Err(InitializePlayerLogisticsError::PositionOutsideContinuousWorld { position });
    }
    if let Some(voxel) = crate::logistics::first_player_body_collision(
        registries.world(),
        state.world(),
        validated.body,
        registries.survival().physiology().locomotion(),
    ) {
        return Err(InitializePlayerLogisticsError::BodyIntersectsSolid { voxel });
    }
    if let Some(voxel) =
        crate::logistics::first_player_temple_caste_collision(registries, state, validated.body)
    {
        return Err(InitializePlayerLogisticsError::BodyIntersectsTempleCaste { voxel });
    }
    if let Some(voxel) =
        crate::logistics::first_player_structure_collision(registries, state, validated.body)
    {
        return Err(InitializePlayerLogisticsError::BodyIntersectsStructure { voxel });
    }
    Ok(validated)
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
