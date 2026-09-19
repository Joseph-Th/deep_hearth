//! Mining-start admission and late-commit error contracts.

mod admission;
mod commit;

pub use admission::MiningStartError;
pub use commit::MiningStartCommitError;
