//! Canonical inventory transaction routing; state records remain passive and privately mutable.

mod egress;
mod reform;
mod relocation;

pub(crate) use egress::{
    MaterialEgressError, ValidatedMaterialEgress, apply_material_egress,
    validate_material_egress_from_selection,
};
pub(crate) use reform::{
    MaterialReformCommitError, MaterialReformError, ValidatedMaterialReform,
    validate_material_reform_from_selection,
};

pub use relocation::{MaterialRelocationCommitError, MaterialRelocationError};
pub(crate) use relocation::{
    ValidatedMaterialRelocation, validate_material_relocation_from_selection,
};

#[cfg(test)]
#[path = "transactions_tests.rs"]
mod tests;

#[cfg(all(test, feature = "test-soak"))]
#[path = "transactions_soak_tests.rs"]
mod soak_tests;
