//! World-space placement and exact same-voxel material transfer.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::inventory::allocation::{
    EmptyStockpileAllocationCommitError, EmptyStockpileAllocationError,
    ValidatedEmptyStockpileAllocation, validate_empty_stockpile_allocation,
};
use crate::inventory::{
    ExplicitConsumptionSelectionError, MaterialLotSelection, MaterialRelocationCommitError,
    MaterialRelocationError, StockpileId, StockpileStorageProfile, ValidatedMaterialRelocation,
    validate_explicit_consumption_selection, validate_material_relocation_from_selection,
};
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::structural::StructuralElementId;

use super::PlayerLogisticsRecord;

/// Failure while allocating one empty loose ground stockpile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroundStockpileAllocationError {
    ZeroCapacity,
    StockpileIdExhausted,
    InventoryRevisionExhausted,
    LogisticsRevisionExhausted,
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
    StaleLogisticsRevision { expected: u64, actual: u64 },
    StaleInventoryRevision { expected: u64, actual: u64 },
}

impl Display for GroundStockpileAllocationCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
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

/// Allocates one empty loose solid-matter stockpile at an explicitly authorized world voxel.
///
/// This creates custody and location only; it creates no matter and makes no claim about terrain,
/// resource generation, or how material arrives there. The caller supplies a finite capacity from
/// its own world/placement contract, while inventory remains authoritative for that capacity.
pub fn validate_allocate_ground_stockpile(
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
    let expected_logistics_revision = state.logistics().revision();
    let next_logistics_revision = expected_logistics_revision
        .checked_add(1)
        .ok_or(GroundStockpileAllocationError::LogisticsRevisionExhausted)?;
    Ok(ValidatedGroundStockpileAllocation {
        expected_logistics_revision,
        next_logistics_revision,
        position,
        allocation,
    })
}

/// Failure while binding an existing unsupported stockpile to one ground voxel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroundStockpilePlacementError {
    UnknownStockpile {
        stockpile: StockpileId,
    },
    PlayerCarried {
        stockpile: StockpileId,
    },
    AlreadyLocated {
        stockpile: StockpileId,
        position: VoxelCoord,
    },
    StructurallyMounted {
        stockpile: StockpileId,
        element: StructuralElementId,
    },
    ReservedInbound {
        stockpile: StockpileId,
        reserved: Mass,
    },
    BusyStorageDismantling {
        stockpile: StockpileId,
    },
    LogisticsRevisionExhausted,
}

impl Display for GroundStockpilePlacementError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownStockpile { stockpile } => {
                write!(formatter, "unknown ground stockpile {}", stockpile.value())
            }
            Self::PlayerCarried { stockpile } => write!(
                formatter,
                "stockpile {} is already carried by the player",
                stockpile.value()
            ),
            Self::AlreadyLocated {
                stockpile,
                position,
            } => write!(
                formatter,
                "stockpile {} is already located at voxel ({},{},{})",
                stockpile.value(),
                position.x(),
                position.y(),
                position.z()
            ),
            Self::StructurallyMounted { stockpile, element } => write!(
                formatter,
                "stockpile {} is mounted to structural element {} and cannot also be a loose ground stockpile",
                stockpile.value(),
                element.value()
            ),
            Self::ReservedInbound {
                stockpile,
                reserved,
            } => write!(
                formatter,
                "stockpile {} has {} mg of reserved inbound matter and cannot change world location",
                stockpile.value(),
                reserved.milligrams()
            ),
            Self::BusyStorageDismantling { stockpile } => write!(
                formatter,
                "stockpile {} participates in active storage dismantling and cannot change world location",
                stockpile.value()
            ),
            Self::LogisticsRevisionExhausted => {
                formatter.write_str("logistics revision space is exhausted")
            }
        }
    }
}

impl Error for GroundStockpilePlacementError {}

/// Failure when a validated ground placement no longer matches owner revisions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroundStockpilePlacementCommitError {
    StaleLogisticsRevision {
        expected: u64,
        actual: u64,
    },
    StaleInventorySupportRevision {
        expected: u64,
        actual: u64,
    },
    UnknownStockpile {
        stockpile: StockpileId,
    },
    PlayerCarried {
        stockpile: StockpileId,
    },
    StructurallyMounted {
        stockpile: StockpileId,
        element: StructuralElementId,
    },
    ReservedInbound {
        stockpile: StockpileId,
        reserved: Mass,
    },
    BusyStorageDismantling {
        stockpile: StockpileId,
    },
}

impl Display for GroundStockpilePlacementCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleLogisticsRevision { expected, actual } => write!(
                formatter,
                "ground placement expected logistics revision {expected} but current revision is {actual}"
            ),
            Self::StaleInventorySupportRevision { expected, actual } => write!(
                formatter,
                "ground placement expected inventory support revision {expected} but current revision is {actual}"
            ),
            Self::UnknownStockpile { stockpile } => write!(
                formatter,
                "ground placement stockpile {} disappeared before commit",
                stockpile.value()
            ),
            Self::PlayerCarried { stockpile } => write!(
                formatter,
                "ground placement stockpile {} became player-carried before commit",
                stockpile.value()
            ),
            Self::StructurallyMounted { stockpile, element } => write!(
                formatter,
                "ground placement stockpile {} became mounted to structural element {} before commit",
                stockpile.value(),
                element.value()
            ),
            Self::ReservedInbound {
                stockpile,
                reserved,
            } => write!(
                formatter,
                "ground placement stockpile {} gained {} mg of reserved inbound matter before commit",
                stockpile.value(),
                reserved.milligrams()
            ),
            Self::BusyStorageDismantling { stockpile } => write!(
                formatter,
                "ground placement stockpile {} entered active storage dismantling before commit",
                stockpile.value()
            ),
        }
    }
}

impl Error for GroundStockpilePlacementCommitError {}

/// Revision-bound placement of an existing loose stockpile into world space.
#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub struct ValidatedGroundStockpilePlacement {
    expected_logistics_revision: u64,
    next_logistics_revision: u64,
    expected_inventory_support_revision: u64,
    stockpile: StockpileId,
    position: VoxelCoord,
}

impl ValidatedGroundStockpilePlacement {
    pub fn commit(self, state: &mut AppState) -> Result<(), GroundStockpilePlacementCommitError> {
        let actual_logistics_revision = state.logistics().revision();
        if actual_logistics_revision != self.expected_logistics_revision {
            return Err(
                GroundStockpilePlacementCommitError::StaleLogisticsRevision {
                    expected: self.expected_logistics_revision,
                    actual: actual_logistics_revision,
                },
            );
        }
        let actual_support_revision = state.inventory().support_revision();
        if actual_support_revision != self.expected_inventory_support_revision {
            return Err(
                GroundStockpilePlacementCommitError::StaleInventorySupportRevision {
                    expected: self.expected_inventory_support_revision,
                    actual: actual_support_revision,
                },
            );
        }
        let record = state.inventory().get_stockpile(self.stockpile).ok_or(
            GroundStockpilePlacementCommitError::UnknownStockpile {
                stockpile: self.stockpile,
            },
        )?;
        if state
            .logistics()
            .player()
            .is_some_and(|player| player.carried_stockpile() == self.stockpile)
        {
            return Err(GroundStockpilePlacementCommitError::PlayerCarried {
                stockpile: self.stockpile,
            });
        }
        if let Some(element) = record.supported_by() {
            return Err(GroundStockpilePlacementCommitError::StructurallyMounted {
                stockpile: self.stockpile,
                element,
            });
        }
        if !record.reserved_inbound().is_zero() {
            return Err(GroundStockpilePlacementCommitError::ReservedInbound {
                stockpile: self.stockpile,
                reserved: record.reserved_inbound(),
            });
        }
        if state
            .player_work()
            .get_storage_dismantling_stockpile_occupant(self.stockpile)
            .is_some()
        {
            return Err(
                GroundStockpilePlacementCommitError::BusyStorageDismantling {
                    stockpile: self.stockpile,
                },
            );
        }
        state.logistics_state_mut().apply_ground_placement(
            self.expected_logistics_revision,
            self.next_logistics_revision,
            self.stockpile,
            self.position,
        );
        Ok(())
    }
}

/// Low-level world/bootstrap authorization for giving one unsupported stockpile a ground location.
///
/// Ordinary pickup/drop actions cannot call this to teleport matter; they only operate on stockpiles
/// already owned by this location map. World generation and placement adapters compose this boundary
/// only after separately proving why the stockpile exists at the requested voxel.
pub fn validate_place_ground_stockpile(
    state: &AppState,
    stockpile: StockpileId,
    position: VoxelCoord,
) -> Result<ValidatedGroundStockpilePlacement, GroundStockpilePlacementError> {
    let record = state
        .inventory()
        .get_stockpile(stockpile)
        .ok_or(GroundStockpilePlacementError::UnknownStockpile { stockpile })?;
    if state
        .logistics()
        .player()
        .is_some_and(|player| player.carried_stockpile() == stockpile)
    {
        return Err(GroundStockpilePlacementError::PlayerCarried { stockpile });
    }
    if let Some(existing) = state.logistics().ground_stockpile_position(stockpile) {
        return Err(GroundStockpilePlacementError::AlreadyLocated {
            stockpile,
            position: existing,
        });
    }
    if let Some(element) = record.supported_by() {
        return Err(GroundStockpilePlacementError::StructurallyMounted { stockpile, element });
    }
    if !record.reserved_inbound().is_zero() {
        return Err(GroundStockpilePlacementError::ReservedInbound {
            stockpile,
            reserved: record.reserved_inbound(),
        });
    }
    if state
        .player_work()
        .get_storage_dismantling_stockpile_occupant(stockpile)
        .is_some()
    {
        return Err(GroundStockpilePlacementError::BusyStorageDismantling { stockpile });
    }
    let expected_logistics_revision = state.logistics().revision();
    let next_logistics_revision = expected_logistics_revision
        .checked_add(1)
        .ok_or(GroundStockpilePlacementError::LogisticsRevisionExhausted)?;
    Ok(ValidatedGroundStockpilePlacement {
        expected_logistics_revision,
        next_logistics_revision,
        expected_inventory_support_revision: state.inventory().support_revision(),
        stockpile,
        position,
    })
}

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
                write!(formatter, "ground transfer selection failed: {error:?}")
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
            Self::Inventory(error) => Some(error),
            Self::PlayerUninitialized
            | Self::GroundStockpileNotLocated { .. }
            | Self::GroundStockpileNotAtPlayer { .. }
            | Self::Selection(_) => None,
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
        .ground_stockpile_position(ground_stockpile)
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
