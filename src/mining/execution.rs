//! Canonical mining execution stages: admission, completion, and reserved-output settlement.

mod cancellation;
mod errors;
mod settlement;
mod start;
mod tick;

pub(crate) use cancellation::{
    MiningCancellationError, MiningCancellationPlan, apply_mining_cancellation,
    decide_mining_cancellation,
};
pub use errors::{MiningStartCommitError, MiningStartError};
pub use settlement::MiningOutputReceipt;
#[cfg(test)]
pub(crate) use settlement::validate_mining_output_settlement_for_test;
pub(crate) use settlement::{
    MiningSettlementError, ValidatedMiningSettlement, validate_awaiting_mining_output_settlement,
    validate_due_mining_output_settlement,
};
pub use start::{ValidatedMiningStart, validate_start_mining};
pub(crate) use tick::{MiningTickPlan, apply_mining_tick, decide_mining_tick};

#[cfg(test)]
#[path = "execution_tests.rs"]
mod tests;
