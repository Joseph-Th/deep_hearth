//! Owns timed material-processing definitions, resolution, durable jobs, and canonical execution.

mod definitions;
mod production_execution;
mod resolution;
mod state;
#[cfg(test)]
mod test_support;

pub use definitions::{ProcessDefinition, ProcessId, ProcessInputPolicy, ProductionRegistry};
pub use production_execution::{
    ProcessCompletion, ProcessOutputLanding, ProcessOutputRoute, ProcessParcelLanding,
    ProductionAvailabilityChange, StartProcessCommitError, StartProcessError,
    ValidatedStartProcess, validate_start_process, validate_start_process_routed,
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
    decide_due_completions, validate_start_manual_process, validate_start_manual_process_routed,
};
pub(crate) use resolution::{sum_lot_spec_mass, validate_repeated_process_inputs};
pub(crate) use state::{validate_loaded_production, validate_loaded_production_schedule_history};

#[cfg(test)]
pub(crate) use resolution::{
    make_test_process_resolution, make_test_process_resolution_with_streams,
};
