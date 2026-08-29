//! Canonical inventory transaction routing; state records remain passive and privately mutable.

mod egress;
mod reform;

#[cfg(any(test, feature = "test-gameplay"))]
mod relocation;
#[cfg(any(test, feature = "test-gameplay"))]
mod transfer;

pub(crate) use egress::{
    MaterialEgressError, ValidatedMaterialEgress, apply_material_egress,
    validate_material_egress_from_selection,
};
pub(crate) use reform::{
    MaterialReformCommitError, MaterialReformError, ValidatedMaterialReform,
    validate_material_reform_from_selection,
};

#[cfg(any(test, feature = "test-gameplay"))]
pub(crate) use relocation::{
    MaterialRelocationCommitError, MaterialRelocationError, ValidatedMaterialRelocation,
    validate_material_relocation_from_selection,
};

#[cfg(test)]
pub(crate) use transfer::{
    MaterialTransferCommitError, MaterialTransferError, MaterialTransferResolution,
    ValidatedMaterialTransfer, validate_material_transfer,
};
#[cfg(all(not(test), feature = "test-gameplay"))]
pub(crate) use transfer::{MaterialTransferResolution, validate_material_transfer};

#[cfg(test)]
#[path = "transactions_tests.rs"]
mod tests;
