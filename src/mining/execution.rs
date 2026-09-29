//! Canonical mining execution stages: admission, completion, and reserved-output claim.

mod cancellation;
mod claim;
mod errors;
mod start;
mod tick;

pub(crate) use cancellation::{
    MiningCancellationError, MiningCancellationPlan, apply_mining_cancellation,
    decide_mining_cancellation,
};
pub use claim::{
    MiningClaimCommitError, MiningClaimError, MiningClaimReceipt, ValidatedMiningClaim,
    validate_claim_mining_output,
};
pub use errors::{MiningStartCommitError, MiningStartError};
pub use start::{ValidatedMiningStart, validate_start_mining};
pub(crate) use tick::{apply_mining_tick, decide_mining_tick};

#[cfg(test)]
#[path = "execution_tests.rs"]
mod tests;
