//! Canonical mining execution stages: admission, completion, and reserved-output settlement.

mod cancellation;
mod claim;
mod errors;
mod start;
mod tick;

pub(crate) use cancellation::{
    MiningCancellationError, MiningCancellationPlan, apply_mining_cancellation,
    decide_mining_cancellation,
};
pub use claim::MiningOutputReceipt;
#[cfg(test)]
pub(crate) use claim::validate_claim_mining_output;
pub(crate) use claim::{
    MiningClaimError, ValidatedMiningClaim, validate_due_mining_output_settlement,
    validate_ready_mining_output_settlement,
};
pub use errors::{MiningStartCommitError, MiningStartError};
pub use start::{ValidatedMiningStart, validate_start_mining};
pub(crate) use tick::{MiningTickPlan, apply_mining_tick, decide_mining_tick};

#[cfg(test)]
#[path = "execution_tests.rs"]
mod tests;
