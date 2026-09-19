//! Read-only planning for due production jobs and their crossed-owner completion effects.

use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::energy::ReleasedEnergyTrace;
use crate::equipment::EquipmentOperationConditionOutcome;
use crate::inventory::{
    AMBIENT_PRESERVATION_MULTIPLIER_PPM, ReservedDepositPlan, ReservedDepositPlanError,
    ReservedDepositRequest, StockpileStoredMassChange, ValidatedStockpileStructuralLoad,
    decide_reserved_deposits, validate_stockpile_stored_mass_changes,
};
use crate::labor::PlayerWork;
use crate::registry::Registries;

use super::super::super::state::{
    ProductionJobId, ProductionJobRecord, ProductionSuspensionReason,
};
use super::availability::decide_availability_changes;
use super::{
    CompletionPlan, CompletionPlanError, PlayerLaborRevisionDependencies,
    ProductionAvailabilityChange, find_availability_change,
};

mod revision_capacity;

use revision_capacity::{build_completion_revision_plan, validate_resumed_job_revision_capacity};

struct DueCompletionPlanning {
    jobs: Vec<ProductionJobId>,
    deposit_requests: Vec<ReservedDepositRequest>,
    equipment_outcomes: Vec<EquipmentOperationConditionOutcome>,
    released_energy_outcomes: Vec<ReleasedEnergyTrace>,
}

impl DueCompletionPlanning {
    fn new(job_count: usize) -> Self {
        Self {
            jobs: Vec::with_capacity(job_count),
            deposit_requests: Vec::new(),
            equipment_outcomes: Vec::new(),
            released_energy_outcomes: Vec::new(),
        }
    }
}

/// Adds the end-of-tick suspension required when fatal survival resolution releases unfinished
/// direct player production. A job completing on the fatal tick is allowed to finish normally.
pub(crate) fn plan_player_death_suspension(
    state: &AppState,
    tick: SimulationTick,
    plan: &mut CompletionPlan,
) -> Result<(), CompletionPlanError> {
    let Some(PlayerWork::ManualProduction { job }) = state.player_work().active() else {
        return Ok(());
    };
    if plan.jobs.contains(&job) {
        return Ok(());
    }
    if let Some(change) = find_availability_change(&plan.availability_changes, job) {
        assert!(
            matches!(change, ProductionAvailabilityChange::Suspended { .. }),
            "active manual production can only have a planned suspension transition"
        );
        return Ok(());
    }
    let record = state.production().get_job(job).unwrap_or_else(|| {
        panic!("player manual-production job disappeared before death suspension")
    });
    assert!(
        record.suspension().is_none(),
        "active manual production cannot already be suspended"
    );
    let remaining_active_time = record
        .completes_at()
        .checked_duration_since(tick)
        .unwrap_or_else(|| panic!("manual production became overdue before death suspension"));
    assert!(
        !remaining_active_time.is_zero(),
        "manual production due on the fatal tick must complete instead of suspending"
    );
    plan.availability_changes
        .push(ProductionAvailabilityChange::Suspended {
            job,
            reason: ProductionSuspensionReason::PlayerLaborUnavailable,
            suspended_at: tick,
            remaining_active_time,
        });
    plan.availability_changes.sort_by_key(|change| change.job());
    if plan.revisions.next_production_revision == plan.revisions.expected_production_revision {
        plan.revisions.next_production_revision = plan
            .revisions
            .expected_production_revision
            .checked_add(1)
            .ok_or(CompletionPlanError::ProductionRevision)?;
    }
    plan.revisions.player_labor_dependencies = Some(PlayerLaborRevisionDependencies {
        expected_player_work_revision: state.player_work().revision(),
        expected_survival_revision: Some(state.survival().revision()),
    });
    Ok(())
}

/// Decides provider availability transitions and all jobs due on one exact tick without mutating
/// production, inventory, equipment, energy, or structure.
pub(crate) fn decide_due_completions(
    registries: &Registries,
    state: &AppState,
    tick: SimulationTick,
) -> Result<CompletionPlan, CompletionPlanError> {
    let (availability_changes, player_labor_dependencies) =
        decide_availability_changes(registries, state)?;
    debug_assert!(
        availability_changes
            .windows(2)
            .all(|pair| pair[0].job() < pair[1].job()),
        "production availability changes must be emitted in stable unique job-ID order"
    );
    let mut due_ids = state.production().jobs_due_at(tick);
    adjust_due_ids_for_availability(&mut due_ids, &availability_changes, tick);
    let mut planning = DueCompletionPlanning::new(due_ids.len());
    for job_id in &due_ids {
        let job = match state.production().get_job(*job_id) {
            Some(job) => job,
            None => panic!(
                "runtime invariant broken: due index references missing production job {}",
                job_id.value()
            ),
        };
        plan_due_job(state, tick, job, &mut planning);
    }
    let revisions = build_completion_revision_plan(
        state,
        !due_ids.is_empty() || !availability_changes.is_empty(),
        !planning.equipment_outcomes.is_empty(),
        !planning.released_energy_outcomes.is_empty(),
        player_labor_dependencies,
    )?;
    let inventory_deposits = decide_reserved_deposits(
        registries,
        state.inventory(),
        tick,
        tick,
        planning.deposit_requests,
    )
    .map_err(|error| match error {
        ReservedDepositPlanError::LotIdExhausted => CompletionPlanError::MaterialLotIds,
        ReservedDepositPlanError::RevisionExhausted => CompletionPlanError::InventoryRevision,
    })?;
    let structural_load = plan_completion_structural_load(registries, state, &inventory_deposits)?;
    let plan = CompletionPlan {
        revisions,
        inventory_deposits,
        availability_changes,
        jobs: planning.jobs,
        equipment_outcomes: planning.equipment_outcomes,
        released_energy_outcomes: planning.released_energy_outcomes,
        structural_load,
    };
    validate_resumed_job_revision_capacity(state, &due_ids, &plan)?;
    Ok(plan)
}

fn adjust_due_ids_for_availability(
    due_ids: &mut std::collections::BTreeSet<ProductionJobId>,
    changes: &[ProductionAvailabilityChange],
    tick: SimulationTick,
) {
    for change in changes {
        match *change {
            ProductionAvailabilityChange::Suspended { job, .. } => {
                due_ids.remove(&job);
            }
            ProductionAvailabilityChange::Resumed {
                job,
                scheduled_completion,
                ..
            } if scheduled_completion == tick => {
                due_ids.insert(job);
            }
            ProductionAvailabilityChange::SuspensionReasonChanged { .. }
            | ProductionAvailabilityChange::Resumed { .. } => {}
        }
    }
}

fn plan_due_job(
    state: &AppState,
    tick: SimulationTick,
    job: &ProductionJobRecord,
    planning: &mut DueCompletionPlanning,
) {
    let storage_age_parts = job
        .material_storage_history()
        .project(tick, AMBIENT_PRESERVATION_MULTIPLIER_PPM)
        .unwrap_or_else(|| {
            panic!(
                "runtime invariant broken: physically reachable production storage history failed to project for job {}",
                job.id().value()
            )
        });
    plan_due_job_outputs(job, storage_age_parts, planning);
    planning.jobs.push(job.id());
    plan_due_job_equipment(state, job, &mut planning.equipment_outcomes);
    if let Some(released) = job.released_energy() {
        planning.released_energy_outcomes.push(released);
    }
}

fn plan_due_job_outputs(
    job: &ProductionJobRecord,
    storage_age_parts: u128,
    planning: &mut DueCompletionPlanning,
) {
    for stream in job.output_streams() {
        planning.deposit_requests.push(ReservedDepositRequest::new(
            stream.destination(),
            stream.outputs().to_vec(),
            storage_age_parts,
        ));
    }
}

fn plan_due_job_equipment(
    state: &AppState,
    job: &ProductionJobRecord,
    outcomes: &mut Vec<EquipmentOperationConditionOutcome>,
) {
    let (Some(provider), Some(after)) = (job.equipment_provider(), job.equipment_condition_after())
    else {
        return;
    };
    if after == provider.condition() {
        return;
    }
    let record = state
        .equipment()
        .get_equipment(provider.equipment())
        .unwrap_or_else(|| {
            panic!(
                "runtime invariant broken: production job {} references missing equipment {}",
                job.id().value(),
                provider.equipment().value()
            )
        });
    assert_eq!(
        record.definition(),
        provider.definition(),
        "runtime invariant broken: occupied equipment definition changed"
    );
    assert_eq!(
        record.condition(),
        provider.condition(),
        "runtime invariant broken: occupied equipment condition changed"
    );
    outcomes.push(EquipmentOperationConditionOutcome::new(
        provider.equipment(),
        provider.condition(),
        after,
    ));
}

fn plan_completion_structural_load(
    registries: &Registries,
    state: &AppState,
    deposits: &ReservedDepositPlan,
) -> Result<Option<ValidatedStockpileStructuralLoad>, CompletionPlanError> {
    let mass_changes = deposits
        .stored_mass_after_by_destination(state.inventory())
        .into_iter()
        .map(|(destination, stored_after)| {
            StockpileStoredMassChange::new(destination, stored_after)
        })
        .collect::<Vec<_>>();
    if mass_changes.is_empty() {
        return Ok(None);
    }
    validate_stockpile_stored_mass_changes(registries, state, mass_changes)
        .map_err(CompletionPlanError::StructuralLoad)
}
