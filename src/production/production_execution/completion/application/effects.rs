//! Infallible production completion effects after the atomicity barrier has passed.

use crate::core::state::AppState;
use crate::energy::{ReleasedEnergyTrace, apply_released_energy_outcomes};
use crate::equipment::EquipmentOperationConditionOutcome;
use crate::inventory::ReservedDepositReceipt;

use super::super::super::start::ProcessOutputRoute;
use super::super::{
    CompletionRevisionPlan, ProcessCompletion, ProcessOutputLanding, ProcessParcelLanding,
    ProductionAvailabilityChange,
};

pub(super) fn apply_availability_changes(
    state: &mut AppState,
    changes: &[ProductionAvailabilityChange],
) {
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

pub(super) fn apply_completion_jobs(
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

pub(super) fn apply_completion_resource_outcomes(
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
