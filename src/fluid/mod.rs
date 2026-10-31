//! Finite fluid ownership, withdrawal, structural loads, and conservation accounting.

mod accounting;
mod definitions;
mod egress;
#[cfg(any(test, feature = "test-gameplay"))]
mod fixture_execution;
mod generation;
mod physics;
mod spring_cycle;
mod state;
mod structural_integration;
mod transfer;

pub use accounting::{
    FluidVolumeAccounting, FluidVolumeAccountingError, calculate_fluid_volume_accounting,
};
pub use definitions::{FluidDefinition, FluidDefinitionId, FluidRegistry};
pub use state::{FluidContents, FluidState, FluidStoreId, FluidStoreRecord, FluidValidationError};
pub use structural_integration::{
    FluidStructuralLoadError, FluidSupportCommitError, FluidSupportError, FluidSupportOutcome,
    ValidatedFluidSupportChange, validate_mount_fluid_store, validate_unmount_fluid_store,
};
pub use transfer::{
    FluidTransferCommitError, FluidTransferError, FluidTransferOutcome, PlayerFluidTransferOption,
    PlayerFluidTransferRoute, ValidatedFluidTransfer, player_fluid_transfer_options,
    validate_gravity_feed_fluid_to_lower_store, validate_player_fluid_transfer_option,
    validate_transfer_fluid_between_local_stores,
};

pub(crate) use egress::{
    FluidEgressCommitError, FluidEgressError, ValidatedFluidEgress, validate_fluid_egress,
};
pub(crate) use generation::{
    FluidGenerationCommitError, FluidGenerationError, GeneratedFluidStoreSpec,
    ValidatedFluidGeneration, ValidatedFluidGenerationBatch, validate_generate_fluid_store,
    validate_generate_fluid_stores,
};
pub(crate) use physics::{
    FluidMassProjectionError, project_fluid_material_mass, project_soaked_spring_water,
};
pub(crate) use spring_cycle::apply_natural_water_cycle;
pub(crate) use state::{validate_loaded_fluid, validate_loaded_natural_spring_locations};
pub(crate) use structural_integration::validate_existing_fluid_load;

#[cfg(test)]
pub(crate) use fixture_execution::add_fluid_store;
#[cfg(any(test, feature = "test-gameplay"))]
pub(crate) use fixture_execution::add_fluid_store_with_contents_for_fixture;
