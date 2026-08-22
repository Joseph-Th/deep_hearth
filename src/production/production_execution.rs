//! Production execution facade; child modules separate process admission from in-flight completion.

mod completion;
mod start;

pub use completion::{ProcessCompletion, ProductionAvailabilityChange};
pub use start::{
    ProcessOutputRoute, StartProcessCommitError, StartProcessError, ValidatedStartProcess,
    validate_start_process, validate_start_process_routed,
};

pub(crate) use completion::{
    CompletionApplication, CompletionCommitError, CompletionPlanError, apply_completion_plan,
    decide_due_completions,
};
pub(crate) use start::validate_start_manual_process;

#[cfg(test)]
#[path = "production_execution_tests.rs"]
mod tests;
