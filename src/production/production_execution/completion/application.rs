//! Atomic application of precomputed production completion and availability decisions.

use std::collections::BTreeSet;

use crate::core::state::AppState;
use crate::energy::{
    ReleasedEnergyTrace, apply_released_energy_outcomes, assert_released_energy_outcomes_available,
};
use crate::equipment::EquipmentOperationConditionOutcome;
use crate::inventory::{ReservedDepositPlan, ReservedDepositReceipt, apply_reserved_deposits};

use super::super::start::ProcessOutputRoute;
use super::{
    CompletionApplication, CompletionCommitError, CompletionPlan, CompletionRevisionPlan,
    PlayerLaborRevisionDependencies, ProcessCompletion, ProcessOutputLanding, ProcessParcelLanding,
    ProductionAvailabilityChange,
};

/// Applies a decided due-job plan in stable job-ID order.
pub(crate) fn apply_completion_plan(
    state: &mut AppState,
    plan: CompletionPlan,
) -> Result<CompletionApplication, CompletionCommitError> {
    let CompletionPlan {
        revisions,
        inventory_deposits,
        availability_changes,
        jobs,
        equipment_outcomes,
        released_energy_outcomes,
        structural_load,
    } = plan;

    validate_completion_inventory_revision(state, &inventory_deposits)?;
    validate_completion_energy_revision(
        state,
        revisions.expected_energy_revision,
        !released_energy_outcomes.is_empty(),
    )?;
    validate_completion_player_labor_revisions(state, revisions.player_labor_dependencies)?;
    validate_completion_equipment_revision(
        state,
        revisions.expected_equipment_revision,
        !equipment_outcomes.is_empty() || !availability_changes.is_empty(),
    )?;
    validate_completion_production_revision(state, revisions.expected_production_revision)?;
    validate_completion_structure_revision(
        state,
        revisions.expected_structure_revision,
        structural_load.is_some() || !availability_changes.is_empty(),
    )?;
    inventory_deposits.assert_matches_state(state.inventory());
    assert_availability_changes_match_state(state, &availability_changes);
    assert_completion_jobs_match_state(state, &jobs);
    if !equipment_outcomes.is_empty() {
        state
            .equipment()
            .assert_operation_condition_outcomes_available(
                revisions.expected_equipment_revision,
                revisions.next_equipment_revision,
                &equipment_outcomes,
            );
    }
    if !released_energy_outcomes.is_empty() {
        assert_released_energy_outcomes_available(
            state.energy(),
            revisions.expected_energy_revision,
            revisions.next_energy_revision,
            &released_energy_outcomes,
        );
    }
    if let Some(structural_load) = structural_load {
        assert_eq!(
            structural_load.expected_revision(),
            revisions.expected_structure_revision,
            "completion structural load must bind the planned structure revision"
        );
        structural_load
            .commit(state)
            .map_err(CompletionCommitError::Structure)?;
    }

    apply_availability_changes(state, &availability_changes);
    let landing_receipts = apply_reserved_deposits(state.inventory_state_mut(), inventory_deposits);
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
    Ok(CompletionApplication {
        completions,
        availability_changes,
    })
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

fn apply_availability_changes(state: &mut AppState, changes: &[ProductionAvailabilityChange]) {
    for change in changes {
        match *change {
            ProductionAvailabilityChange::Suspended {
                job,
                reason,
                suspended_at,
                remaining_active_time,
            } => state.production_state_mut().suspend_job(
                job,
                suspended_at,
                remaining_active_time,
                reason,
            ),
            ProductionAvailabilityChange::SuspensionReasonChanged {
                job,
                previous,
                reason,
            } => state
                .production_state_mut()
                .change_suspension_reason(job, previous, reason),
            ProductionAvailabilityChange::Resumed {
                job,
                resumed_at,
                scheduled_completion,
                ..
            } => state
                .production_state_mut()
                .resume_job(job, resumed_at, scheduled_completion),
        }
    }
}

fn apply_completion_jobs(
    state: &mut AppState,
    jobs: Vec<crate::production::ProductionJobId>,
    landing_receipts: Vec<ReservedDepositReceipt>,
) -> Vec<ProcessCompletion> {
    let expected_landings = jobs
        .iter()
        .map(|job| {
            state
                .production()
                .get_job(*job)
                .unwrap_or_else(|| unreachable!("validated completion job remains available"))
                .output_streams()
                .len()
        })
        .sum::<usize>();
    assert_eq!(
        landing_receipts.len(),
        expected_landings,
        "production completion must receive one inventory landing receipt per output stream"
    );
    let mut landing_receipts = landing_receipts.into_iter();
    let mut completions = Vec::with_capacity(jobs.len());
    for job in jobs {
        let removed = state.production_state_mut().remove_job(job);
        let process = removed.process();
        let output_streams = removed.output_streams;
        let routes = output_streams
            .iter()
            .map(|stream| ProcessOutputRoute::new(stream.id, stream.destination))
            .collect::<Vec<_>>();
        let landings = output_streams
            .iter()
            .map(|stream| {
                let receipt = landing_receipts.next().unwrap_or_else(|| {
                    unreachable!("validated completion landing count was checked above")
                });
                assert_eq!(
                    receipt.destination(),
                    stream.destination,
                    "inventory landing receipt destination must match its production output route"
                );
                let lot_ids = receipt.into_lot_ids();
                assert_eq!(
                    lot_ids.len(),
                    stream.outputs.len(),
                    "production completion must receive one landing identity per output parcel"
                );
                let parcels = stream
                    .outputs
                    .iter()
                    .cloned()
                    .zip(lot_ids)
                    .map(|(output, lot)| ProcessParcelLanding { lot, output })
                    .collect();
                ProcessOutputLanding {
                    stream: stream.id,
                    destination: stream.destination,
                    parcels,
                }
            })
            .collect::<Vec<_>>();
        completions.push(ProcessCompletion {
            job,
            process,
            routes,
            landings,
        });
    }
    assert!(
        landing_receipts.next().is_none(),
        "production completion left an unmatched inventory landing receipt"
    );
    completions
}

fn apply_completion_resource_outcomes(
    state: &mut AppState,
    has_completions: bool,
    revisions: &CompletionRevisionPlan,
    equipment_outcomes: &[EquipmentOperationConditionOutcome],
    released_energy_outcomes: &[ReleasedEnergyTrace],
) {
    if !has_completions {
        return;
    }
    if !equipment_outcomes.is_empty() {
        state
            .equipment_state_mut()
            .apply_operation_condition_outcomes(
                revisions.expected_equipment_revision,
                revisions.next_equipment_revision,
                equipment_outcomes,
            );
    }
    if !released_energy_outcomes.is_empty() {
        apply_released_energy_outcomes(
            state.energy_state_mut(),
            revisions.expected_energy_revision,
            revisions.next_energy_revision,
            released_energy_outcomes,
        );
    }
}
