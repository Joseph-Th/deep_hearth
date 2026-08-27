//! Finite fluid ownership, withdrawal, structural loads, and exact flow integration.

mod accounting;
mod definitions;
mod egress;
#[cfg(any(test, feature = "test-gameplay"))]
mod fixture_execution;
mod integration;
mod state;
mod structural_integration;

pub use accounting::{
    FluidVolumeAccounting, FluidVolumeAccountingError, calculate_fluid_volume_accounting,
};
pub use definitions::{FluidDefinition, FluidDefinitionId, FluidRegistry};
pub use integration::{FlowIntegration, FlowIntegrationError, FlowRemainder, integrate_flow};
pub use state::{FluidContents, FluidState, FluidStoreId, FluidStoreRecord, FluidValidationError};
pub use structural_integration::{
    FluidStructuralLoadError, FluidSupportCommitError, FluidSupportError, FluidSupportOutcome,
    ValidatedFluidSupportChange, validate_mount_fluid_store, validate_unmount_fluid_store,
};

pub(crate) use egress::{
    FluidEgressCommitError, FluidEgressError, ValidatedFluidEgress, validate_fluid_egress,
};
pub(crate) use state::validate_loaded_fluid;
pub(crate) use structural_integration::validate_existing_fluid_load;

#[cfg(test)]
pub(crate) use fixture_execution::add_fluid_store;
#[cfg(any(test, feature = "test-gameplay"))]
pub(crate) use fixture_execution::add_fluid_store_with_contents_for_fixture;
