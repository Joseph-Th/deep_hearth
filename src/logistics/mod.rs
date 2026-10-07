//! Persistent player world location and carried inventory custody.
//!
//! Logistics owns *where* the player and carried custody are. Inventory remains authoritative for
//! the carried stockpile's lots, capacity, temperature, preservation, and matter accounting.
//! Logistics intentionally does not expose movement or arbitrary stockpile transfer: those need
//! world access/path authorization rather than inventory-level teleportation.

mod access;
#[cfg(feature = "test-gameplay")]
mod fixture;
mod fluid;
mod ground;
mod initialization;
mod state;
mod validation;

pub use access::{
    PlayerEnergyStoreAccessError, PlayerEquipmentAccessError, PlayerFluidStoreAccessError,
    PlayerStockpileAccessError, validate_player_energy_store_access,
    validate_player_equipment_access, validate_player_fluid_store_access,
    validate_player_stockpile_access,
};
#[cfg(feature = "test-gameplay")]
pub(crate) use fixture::{place_energy_store_for_fixture, place_equipment_for_fixture};
pub use fluid::{
    FluidStorePlacementCommitError, FluidStorePlacementError, ValidatedFluidStorePlacement,
    validate_place_fluid_store,
};
pub use ground::{
    GroundMaterialTransferCommitError, GroundMaterialTransferError,
    GroundStockpileAllocationCommitError, GroundStockpileAllocationError,
    GroundStockpilePlacementCommitError, GroundStockpilePlacementError,
    PlayerGroundStockpileAllocationError, ValidatedGroundMaterialTransfer,
    ValidatedGroundStockpileAllocation, ValidatedGroundStockpilePlacement,
    validate_allocate_ground_stockpile, validate_allocate_player_ground_stockpile,
    validate_drop_to_ground, validate_pickup_from_ground, validate_place_ground_stockpile,
};
pub use initialization::{
    InitializePlayerLogisticsCommitError, InitializePlayerLogisticsError,
    ValidatedPlayerLogisticsInitialization, validate_initialize_player_logistics,
};
pub use state::{
    LogisticsState, PlayerCarryingAssessment, PlayerLogisticsRecord, assess_player_carrying,
};
pub use validation::LogisticsValidationError;
pub(crate) use validation::validate_loaded_logistics;

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
