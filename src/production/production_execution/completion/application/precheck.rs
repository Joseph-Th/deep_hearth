//! Read-only atomicity barrier for production completion application.

use std::collections::BTreeSet;

use crate::core::state::AppState;
use crate::energy::assert_released_energy_outcomes_available;
use crate::inventory::ReservedDepositPlan;

use super::super::{
    CompletionCommitError, CompletionPlan, PlayerLaborRevisionDependencies,
    ProductionAvailabilityChange,
};

pub(super) fn precheck_completion_application(
    state: &AppState,
    plan: &CompletionPlan,
) -> Result<(), CompletionCommitError> {
    validate_completion_inventory_revision(state, &plan.inventory_deposits)?;
    validate_completion_energy_revision(
        state,
        plan.revisions.expected_energy_revision,
        !plan.released_energy_outcomes.is_empty(),
    )?;
    validate_completion_player_labor_revisions(state, plan.revisions.player_labor_dependencies)?;
    validate_completion_equipment_revision(
        state,
        plan.revisions.expected_equipment_revision,
        !plan.equipment_outcomes.is_empty() || !plan.availability_changes.is_empty(),
    )?;
    validate_completion_production_revision(state, plan.revisions.expected_production_revision)?;
    validate_completion_structure_revision(
        state,
        plan.revisions.expected_structure_revision,
        plan.structural_load.is_some() || !plan.availability_changes.is_empty(),
    )?;
    plan.inventory_deposits
        .assert_matches_state(state.inventory());
    assert_availability_changes_match_state(state, &plan.availability_changes);
    assert_completion_jobs_match_state(state, &plan.jobs);
    if !plan.equipment_outcomes.is_empty() {
        state
            .equipment()
            .assert_operation_condition_outcomes_available(
                plan.revisions.expected_equipment_revision,
                plan.revisions.next_equipment_revision,
                &plan.equipment_outcomes,
            );
    }
    if !plan.released_energy_outcomes.is_empty() {
        assert_released_energy_outcomes_available(
            state.energy(),
            plan.revisions.expected_energy_revision,
            plan.revisions.next_energy_revision,
            &plan.released_energy_outcomes,
        );
    }
    if let Some(structural_load) = plan.structural_load.as_ref() {
        assert_eq!(
            structural_load.expected_revision(),
            plan.revisions.expected_structure_revision,
            "completion structural load must bind the planned structure revision"
        );
    }
    Ok(())
}

fn assert_completion_jobs_match_state(
    state: &AppState,
    jobs: &[crate::production::ProductionJobId],
) {
    let mut seen_jobs = BTreeSet::new();
    for job in jobs {
        assert!(
            seen_jobs.insert(*job),
            "completion plan contains duplicate production job {}",
            job.value()
        );
        state
            .production()
            .get_job(*job)
            .unwrap_or_else(|| panic!("validated completion references missing production job"));
        state.production().assert_job_removable(*job);
    }
}

fn assert_availability_changes_match_state(
    state: &AppState,
    changes: &[ProductionAvailabilityChange],
) {
    let mut seen_jobs = BTreeSet::new();
    for change in changes {
        let job = match *change {
            ProductionAvailabilityChange::Suspended {
                job,
                suspended_at,
                remaining_active_time,
                ..
            } => {
                state.production().assert_suspend_job_available(
                    job,
                    suspended_at,
                    remaining_active_time,
                );
                job
            }
            ProductionAvailabilityChange::SuspensionReasonChanged {
                job,
                previous,
                reason,
            } => {
                state
                    .production()
                    .assert_suspension_reason_change_available(job, previous, reason);
                job
            }
            ProductionAvailabilityChange::Resumed {
                job,
                resumed_at,
                scheduled_completion,
                ..
            } => {
                let _ = state.production().assert_resume_job_available(
                    job,
                    resumed_at,
                    scheduled_completion,
                );
                job
            }
        };
        assert!(
            seen_jobs.insert(job),
            "completion availability plan contains duplicate production job {}",
            job.value()
        );
    }
}

fn validate_completion_inventory_revision(
    state: &AppState,
    inventory_deposits: &ReservedDepositPlan,
) -> Result<(), CompletionCommitError> {
    let expected = inventory_deposits.expected_revision();
    let actual = state.inventory().revision();
    if actual != expected {
        return Err(CompletionCommitError::InventoryStale { expected, actual });
    }
    Ok(())
}

fn validate_completion_energy_revision(
    state: &AppState,
    expected: u64,
    required: bool,
) -> Result<(), CompletionCommitError> {
    if !required {
        return Ok(());
    }
    let actual = state.energy().revision();
    if actual != expected {
        return Err(CompletionCommitError::EnergyRevisionConflict { expected, actual });
    }
    Ok(())
}

fn validate_completion_player_labor_revisions(
    state: &AppState,
    dependencies: Option<PlayerLaborRevisionDependencies>,
) -> Result<(), CompletionCommitError> {
    let Some(dependencies) = dependencies else {
        return Ok(());
    };
    let actual_player_work_revision = state.player_work().revision();
    if actual_player_work_revision != dependencies.expected_player_work_revision {
        return Err(CompletionCommitError::PlayerWorkRevisionConflict {
            expected: dependencies.expected_player_work_revision,
            actual: actual_player_work_revision,
        });
    }
    let Some(expected_survival_revision) = dependencies.expected_survival_revision else {
        return Ok(());
    };
    let actual_survival_revision = state.survival().revision();
    if actual_survival_revision != expected_survival_revision {
        return Err(CompletionCommitError::SurvivalRevisionConflict {
            expected: expected_survival_revision,
            actual: actual_survival_revision,
        });
    }
    Ok(())
}

fn validate_completion_equipment_revision(
    state: &AppState,
    expected: u64,
    required: bool,
) -> Result<(), CompletionCommitError> {
    if !required {
        return Ok(());
    }
    let actual = state.equipment().revision();
    if actual != expected {
        return Err(CompletionCommitError::EquipmentRevisionConflict { expected, actual });
    }
    Ok(())
}

fn validate_completion_production_revision(
    state: &AppState,
    expected: u64,
) -> Result<(), CompletionCommitError> {
    let actual = state.production().revision();
    if actual != expected {
        return Err(CompletionCommitError::ProductionRevisionChanged { expected, actual });
    }
    Ok(())
}

fn validate_completion_structure_revision(
    state: &AppState,
    expected: u64,
    required: bool,
) -> Result<(), CompletionCommitError> {
    if !required {
        return Ok(());
    }
    let actual = state.structures().revision();
    if actual != expected {
        return Err(CompletionCommitError::StructureRevisionConflict { expected, actual });
    }
    Ok(())
}
