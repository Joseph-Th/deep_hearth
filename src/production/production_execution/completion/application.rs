//! Atomic application of precomputed production completion and availability decisions.

use crate::core::state::AppState;

use super::{CompletionApplication, CompletionCommitError, CompletionPlan};

mod effects;
mod precheck;

use effects::{
    apply_availability_changes, apply_completion_jobs, apply_completion_resource_outcomes,
};
use precheck::precheck_completion_application;

/// Applies a decided due-job plan in stable job-ID order.
pub(crate) fn apply_completion_plan(
    state: &mut AppState,
    plan: CompletionPlan,
) -> Result<CompletionApplication, CompletionCommitError> {
    if !plan.has_authoritative_effects() {
        if state
            .production()
            .physical_availability_dependencies_changed(plan.availability_dependency_revisions)
        {
            state
                .production_state_mut()
                .record_physical_availability_dependencies(plan.availability_dependency_revisions);
        }
        return Ok(CompletionApplication {
            completions: Vec::new(),
            availability_changes: Vec::new(),
        });
    }
    precheck_completion_application(state, &plan)?;

    let CompletionPlan {
        revisions,
        availability_dependency_revisions,
        inventory_deposits,
        availability_changes,
        jobs,
        equipment_outcomes,
        released_energy_outcomes,
        structural_load,
    } = plan;

    // This is the first mutation. Every recoverable cross-owner conflict is checked above.
    if let Some(structural_load) = structural_load {
        structural_load
            .commit(state)
            .map_err(CompletionCommitError::Structure)?;
    }

    apply_availability_changes(state, &availability_changes);
    let landing_receipts =
        crate::inventory::apply_reserved_deposits(state.inventory_state_mut(), inventory_deposits);
    let completions = apply_completion_jobs(state, jobs, landing_receipts);
    apply_completion_resource_outcomes(
        state,
        !completions.is_empty(),
        &revisions,
        &equipment_outcomes,
        &released_energy_outcomes,
    );
    if !completions.is_empty() || !availability_changes.is_empty() {
        state
            .production_state_mut()
            .apply_revision(revisions.next_production_revision);
    }
    if state
        .production()
        .physical_availability_dependencies_changed(availability_dependency_revisions)
    {
        state
            .production_state_mut()
            .record_physical_availability_dependencies(availability_dependency_revisions);
    }
    Ok(CompletionApplication {
        completions,
        availability_changes,
    })
}
