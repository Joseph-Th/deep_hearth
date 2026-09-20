//! Process input requirements and runtime material-lot creation specifications.

mod input;
mod specification;

pub use input::{MaterialInputSpec, MaterialInputSpecError};
pub use specification::{MaterialLotSpec, MaterialLotSpecError};

#[cfg(test)]
#[path = "lot_tests.rs"]
mod tests;
