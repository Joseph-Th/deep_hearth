//! Process input requirements and runtime material-lot creation specifications.

mod input;
mod specification;

pub use input::{MaterialInputSpec, MaterialInputSpecError};
pub use specification::{MaterialLotSpec, MaterialLotSpecError};

#[cfg(all(
    test,
    any(not(feature = "test-unit-shard"), feature = "test-unit-material")
))]
#[path = "lot_tests.rs"]
mod tests;
