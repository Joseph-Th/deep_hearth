//! Tool- and labor-gated finite geological extraction.

mod definitions;
mod execution;
mod order;
mod physics;
mod state;
mod target_resolution;
mod validation;

pub use definitions::{MiningMethodDefinition, MiningMethodId, MiningRegistry};
pub(crate) use execution::{
    MiningCancellationError, MiningCancellationPlan, MiningClaimError, MiningTickPlan,
    ValidatedMiningClaim, apply_mining_cancellation, apply_mining_tick, decide_mining_cancellation,
    decide_mining_tick, validate_due_mining_output_settlement,
    validate_ready_mining_output_settlement,
};
pub use execution::{
    MiningOutputReceipt, MiningStartCommitError, MiningStartError, ValidatedMiningStart,
    validate_start_mining,
};
pub use order::{
    MiningOrderError, MiningOrderRequest, MiningOrderResolution, resolve_mining_order,
};
pub use physics::MiningPhysicsError;
pub(crate) use physics::resolve_mining_physics;
pub use state::{MiningJobId, MiningJobRecord, MiningState, MiningValidationError};
pub(crate) use state::{serialize_mining_state, validate_loaded_mining};
pub use target_resolution::{
    MiningTargetRequest, MiningTargetResolution, MiningTargetResolutionError, resolve_mining_target,
};
pub use validation::MiningJobValidationError;
pub(crate) use validation::validate_loaded_mining_jobs;
