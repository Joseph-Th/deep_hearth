//! Finite loose matter in world space, actor-local observation, and direct player gathering.

mod creation;
mod gathering;
mod observation;
mod state;

#[cfg(test)]
pub(crate) use creation::insert_surface_resource_for_test;
pub(crate) use creation::{
    SurfaceResourceCreationCommitError, SurfaceResourceCreationError, SurfaceResourceSpec,
    SurfaceResourceSpecError, ValidatedSurfaceResourceCreation, validate_create_surface_resources,
    validate_create_surface_resources_at,
};
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
pub use observation::{AdjacentSurfaceResource, available_adjacent_surface_resources};
pub use state::{
    SurfaceResourceId, SurfaceResourceLifecycle, SurfaceResourceRecord,
    SurfaceResourceValidationError,
};
pub(crate) use state::{
    SurfaceResourceState, serialize_surface_resource_state, validate_loaded_surface_resources,
};

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
