//! Canonical mining execution stages: admission, completion, and reserved-output claim.

mod claim;
mod errors;
mod start;
mod tick;

pub use claim::{
    MiningClaimCommitError, MiningClaimError, MiningClaimReceipt, ValidatedMiningClaim,
    validate_claim_mining_output,
};
pub use errors::{MiningStartCommitError, MiningStartError};
pub use start::{ValidatedMiningStart, validate_start_mining};
pub(crate) use tick::{MiningTickError, apply_mining_tick, decide_mining_tick};

#[cfg(test)]
#[path = "execution_tests.rs"]
mod tests;
