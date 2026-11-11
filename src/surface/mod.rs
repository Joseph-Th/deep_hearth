//! Finite loose matter in world space, actor-local observation, and direct player gathering.

mod creation;
mod drop_projection;
mod footing;
mod gathering;
mod ground_stockpile_raid;
mod material_drop;
mod observation;
mod pickup;
mod scavenging;
mod state;

#[cfg(test)]
pub(crate) use creation::insert_surface_resource_for_test;
pub(crate) use creation::{
    SurfaceResourceCreationCommitError, SurfaceResourceCreationError, SurfaceResourceSpec,
    SurfaceResourceSpecError, ValidatedSurfaceResourceCreation, validate_create_surface_resources,
    validate_create_surface_resources_after_tree_felling_at, validate_create_surface_resources_at,
};
pub use drop_projection::{PlayerWorldDropProjection, available_player_world_drops};
pub(crate) use footing::{OBSTRUCTING_RESIDUE_COMPACT_DEPTH_UM, exposed_residue_compact_depth_um};
pub use gathering::{
    PlayerSurfaceFoodValue, PlayerSurfaceGatheringOption, SurfaceGatheringCommitError,
    SurfaceGatheringError, SurfaceGatheringOutcome, SurfaceGatheringRequest,
    ValidatedSurfaceGatheringStart, player_surface_gathering_options,
    validate_start_player_surface_gathering, validate_start_player_surface_gathering_option,
    validate_start_surface_gathering,
};
pub(crate) use gathering::{
    SurfaceGatheringCancellationPlan, SurfaceGatheringTickError, SurfaceGatheringTickPlan,
    apply_surface_gathering_cancellation, apply_surface_gathering_tick,
    assess_player_surface_gathering_option_admission, assess_player_surface_gathering_reservation,
    decide_surface_gathering_cancellation, decide_surface_gathering_tick,
    validate_start_player_surface_gathering_option_control_rate,
};
pub(crate) use ground_stockpile_raid::{
    ScavengedGroundStockpileRequest, scavenge_unenclosed_ground_stockpile_material,
};
pub use material_drop::{
    PlayerMaterialDropCommitError, PlayerMaterialDropError, PlayerMaterialDropOutcome,
    ValidatedPlayerMaterialDrop, validate_drop_player_material,
};
pub use observation::{AdjacentSurfaceResource, available_adjacent_surface_resources};
pub use pickup::SurfaceDropPickupOutcome;
pub(crate) use pickup::{
    apply_automatic_surface_drop_pickup, automatic_surface_pickup_requires_control_stepping,
    decide_automatic_surface_drop_pickup,
};

pub(crate) use scavenging::{
    ScavengedPortionRequest, abrade_scavenged_tool_head, forage_food_to_adjacent_den,
    relocate_scavenged_drop_step, resource_reserved_for_player_gathering,
    split_bulk_scavenged_drop_step, split_bulk_scavenged_drop_with_head_wear,
};

pub use state::{
    SurfaceResourceId, SurfaceResourceKind, SurfaceResourceLifecycle, SurfaceResourceRecord,
    SurfaceResourceValidationError,
};
pub(crate) use state::{
    SurfaceResourceState, serialize_surface_resource_state, validate_loaded_surface_resources,
};

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "pickup_tests.rs"]
mod pickup_tests;
