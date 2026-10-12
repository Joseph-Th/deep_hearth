//! Finite loose matter in world space, actor-local observation, and direct player gathering.

mod gathering;
mod generation;
mod observation;
mod state;

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
#[cfg(test)]
pub(crate) use generation::insert_generated_surface_resource;
pub(crate) use generation::{
    GeneratedSurfaceResourceSpec, GeneratedSurfaceResourceSpecError, SurfaceGenerationCommitError,
    SurfaceGenerationError, ValidatedSurfaceGeneration, validate_generate_surface_resources,
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
