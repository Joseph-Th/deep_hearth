//! Owns ore/material preparation definitions, shared physics, and execution APIs.

mod comminution_execution;
mod definitions;
mod manual_physics;
mod order;
mod planning;
mod powered_physics;
mod registry;
mod screening_execution;
mod separation_execution;
mod validation;

pub use order::{
    PoweredOreOrderBatch, PoweredOreOrderError, PoweredOreOrderMaintenancePolicy,
    PoweredOreOrderRequest, PoweredOreOrderResolution, project_powered_ore_order,
};
pub use planning::{
    PoweredOreMassConstraint, PoweredOreMassEnvelope, PoweredOreMassEnvelopeError,
    assess_powered_ore_mass_envelope,
};
pub use powered_physics::{PoweredOreBottleneck, PoweredOreJobValidationError};
pub use registry::OreProcessingRegistry;

pub use comminution_execution::{
    ComminutionBatchError, ComminutionJobValidationError, ComminutionRequest,
    ComminutionResolutionError, ManualComminutionCommitError, ManualComminutionRequest,
    ManualComminutionResolutionError, ResolvedComminution, ResolvedManualComminution,
    StartManualComminutionError, ValidatedManualComminutionStart, resolve_comminution_process,
    resolve_manual_comminution_process, validate_start_manual_comminution,
};

pub(crate) use comminution_execution::validate_loaded_comminution_job;

pub use definitions::{
    ComminutionProcessDefinition, ConstituentRecoveryProfile,
    ConstituentSeparationProcessDefinition, ManualComminutionProcessDefinition,
    ManualConstituentSeparationProcessDefinition, ManualOreProcessProfile,
    PoweredOreProcessProfile, ScreeningProcessDefinition,
};
pub use manual_physics::{
    ManualOreJobValidationError, ManualOrePhysicsError, project_manual_ore_duration,
};

pub use separation_execution::{
    ConstituentSeparationBatchError, ConstituentSeparationJobValidationError,
    ConstituentSeparationRequest, ConstituentSeparationResolutionError,
    ManualConstituentSeparationCommitError, ManualConstituentSeparationRequest,
    ManualConstituentSeparationResolutionError, ResolvedConstituentSeparation,
    ResolvedManualConstituentSeparation, StartManualConstituentSeparationError,
    ValidatedManualConstituentSeparationStart, resolve_constituent_separation_process,
    resolve_manual_constituent_separation_process, validate_start_manual_constituent_separation,
};

pub(crate) use separation_execution::validate_loaded_constituent_separation_job;

pub use screening_execution::{
    ResolvedScreening, ScreeningBatchError, ScreeningJobValidationError, ScreeningRequest,
    ScreeningResolutionError, resolve_representable_screening_mass, resolve_screening_process,
};

pub(crate) use screening_execution::validate_loaded_screening_job;

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
