//! Cross-owner persistence validation for mining job semantics and resource references.

use crate::core::state::AppState;
use crate::registry::Registries;

mod error;
mod history;
mod job;

pub use error::MiningJobValidationError;

pub(crate) fn validate_loaded_mining_jobs(
    registries: &Registries,
    state: &AppState,
) -> Result<(), MiningJobValidationError> {
    for job in state.mining().jobs() {
        job::validate_loaded_mining_job(registries, state, job)?;
    }
    history::validate_retained_work_intervals(state)?;
    history::validate_retained_deposit_history(state)
}

#[cfg(test)]
use job::validate_mining_equipment_portability;

#[cfg(test)]
#[path = "validation_tests.rs"]
mod tests;
