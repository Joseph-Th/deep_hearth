//! Finite loose matter in world space, actor-local observation, and direct player gathering.

mod creation;
mod drop_projection;
mod gathering;
mod material_drop;
mod observation;
mod pickup;
mod state;

#[cfg(test)]
pub(crate) use creation::insert_surface_resource_for_test;
pub(crate) use creation::{
    SurfaceResourceCreationCommitError, SurfaceResourceCreationError, SurfaceResourceSpec,
    SurfaceResourceSpecError, ValidatedSurfaceResourceCreation, validate_create_surface_resources,
    validate_create_surface_resources_at,
};
pub use drop_projection::{PlayerWorldDropProjection, available_player_world_drops};
pub(crate) use gathering::{
    SurfaceGatheringCancellationPlan, SurfaceGatheringTickError, SurfaceGatheringTickPlan,
    apply_surface_gathering_cancellation, apply_surface_gathering_tick,
    decide_surface_gathering_cancellation, decide_surface_gathering_tick,
};
pub use gathering::{
    SurfaceGatheringCommitError, SurfaceGatheringError, SurfaceGatheringOutcome,
    SurfaceGatheringRequest, ValidatedSurfaceGatheringStart,
    validate_start_player_surface_gathering, validate_start_surface_gathering,
};
pub use material_drop::{
    PlayerMaterialDropCommitError, PlayerMaterialDropError, PlayerMaterialDropOutcome,
    ValidatedPlayerMaterialDrop, validate_drop_player_material,
};
pub use observation::{AdjacentSurfaceResource, available_adjacent_surface_resources};
pub use pickup::SurfaceDropPickupOutcome;
pub(crate) use pickup::{
    apply_automatic_surface_drop_pickup, decide_automatic_surface_drop_pickup,
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
