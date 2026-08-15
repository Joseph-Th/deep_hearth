//! Timed material-processing subsystem; definitions are immutable, state owns durable jobs, and execution mutates canonically.

mod definitions;
mod production_execution;
mod resolution;
mod state;

pub use definitions::{ProcessDefinition, ProcessId, ProcessInputPolicy, ProductionRegistry};
pub use production_execution::{
    ProcessCompletion, ProcessOutputRoute, ProductionAvailabilityChange, StartProcessCommitError,
    StartProcessError, ValidatedStartProcess, validate_start_process,
    validate_start_process_routed,
};
pub use resolution::{
    ProcessInputError, ProcessOutputStream, ProcessOutputStreamId, ProcessResolution,
    ProcessResolutionError, ValidatedProcessInputs, validate_process_inputs,
    validate_selected_process_inputs,
};
pub use state::{
    ProductionJobId, ProductionJobRecord, ProductionOccupancyRelease, ProductionOutputStream,
    ProductionState, ProductionSuspension, ProductionSuspensionReason, ProductionValidationError,
};

pub(crate) use production_execution::{
    CompletionApplication, CompletionCommitError, CompletionPlanError, apply_completion_plan,
    decide_due_completions,
};
pub(crate) use resolution::sum_lot_spec_mass;
pub(crate) use state::validate_loaded_production;

#[cfg(test)]
pub(crate) use resolution::{
    make_test_process_resolution, make_test_process_resolution_with_streams,
};
