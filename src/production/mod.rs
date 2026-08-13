//! Timed material-processing subsystem; definitions are immutable, state owns durable jobs, and execution mutates canonically.

mod definitions;
mod production_execution;
mod resolution;
mod state;

pub use definitions::{ProcessDefinition, ProcessId, ProductionRegistry};
pub use production_execution::{
    ProcessCompletion, StartProcessCommitError, StartProcessError, ValidatedStartProcess,
    validate_start_process,
};
pub use resolution::ProcessResolution;
pub use state::{ProductionJobId, ProductionJobRecord, ProductionState, ProductionValidationError};

pub(crate) use production_execution::{
    CompletionCommitError, CompletionPlanError, apply_completion_plan, decide_due_completions,
};
pub(crate) use resolution::sum_lot_spec_mass;
pub(crate) use state::validate_loaded_production;

#[cfg(test)]
pub(crate) use resolution::make_test_process_resolution;
