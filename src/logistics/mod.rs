//! Persistent player world location and carried inventory custody.
//!
//! Logistics owns *where* the player and carried custody are. Inventory remains authoritative for
//! the carried stockpile's lots, capacity, temperature, preservation, and matter accounting.
//! Logistics also owns explicit persisted face-adjacent traversal authorization. General terrain
//! geometry, pathfinding, route cost, haulage, and arbitrary stockpile transport remain absent.

mod access;
#[cfg(feature = "test-gameplay")]
mod fixture;
mod fluid;
mod ground;
mod initialization;
mod movement;
mod state;
mod traversal;
mod validation;

pub(crate) use access::player_step_blocks_carried_load_increase;
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
#[cfg(test)]
pub(crate) use initialization::validate_initialize_player_logistics;
pub(crate) use initialization::{
    InitializePlayerLogisticsCommitError, InitializePlayerLogisticsError,
    ValidatedPlayerLogisticsInitialization, validate_player_logistics_initialization,
};
pub use movement::{
    PlayerStepCommitError, PlayerStepError, PlayerStepOutcome, ValidatedPlayerStepStart,
    validate_start_player_step,
};
pub(crate) use movement::{apply_player_step_tick, decide_player_step_tick};
#[cfg(test)]
pub(crate) use state::add_traversable_edge_for_test;
pub use state::{
    LogisticsState, PlayerCarryingAssessment, PlayerLogisticsRecord, assess_player_carrying,
};
pub(crate) use traversal::TraversalEdge;
pub use validation::LogisticsValidationError;
pub(crate) use validation::validate_loaded_logistics;

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
