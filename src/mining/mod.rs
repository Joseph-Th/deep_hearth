//! Tool- and labor-gated finite geological extraction.

mod definitions;
mod execution;
mod physics;
mod state;
mod validation;

pub use definitions::{MiningMethodDefinition, MiningMethodId, MiningRegistry};
pub use execution::{
    MiningClaimCommitError, MiningClaimError, MiningStartCommitError, MiningStartError,
    ValidatedMiningClaim, ValidatedMiningStart, validate_claim_mining_output,
    validate_start_mining,
};
pub(crate) use execution::{MiningTickError, apply_mining_tick, decide_mining_tick};
pub(crate) use state::validate_loaded_mining;
pub use state::{MiningJobId, MiningJobRecord, MiningState, MiningValidationError};
pub use validation::MiningJobValidationError;
pub(crate) use validation::validate_loaded_mining_jobs;
