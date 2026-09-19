//! Future revision-capacity forecasting for production completion and resume plans.

use std::collections::BTreeSet;

use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::inventory::{StockpileStructuralLoadError, ValidatedStockpileStructuralLoad};
use crate::production::state::{ProductionJobId, ProductionJobRecord};
use crate::structural::StructuralMutationError;

use super::super::{
    CompletionPlan, CompletionPlanError, CompletionRevisionPlan, PlayerLaborRevisionDependencies,
    ProductionAvailabilityChange, find_availability_change,
};

fn planned_revision(
    expected: u64,
    changed: bool,
    exhausted: CompletionPlanError,
) -> Result<u64, CompletionPlanError> {
    if !changed {
        return Ok(expected);
    }
    expected.checked_add(1).ok_or(exhausted)
}

pub(super) fn build_completion_revision_plan(
    state: &AppState,
    production_changed: bool,
    equipment_changed: bool,
    energy_changed: bool,
    player_labor_dependencies: Option<PlayerLaborRevisionDependencies>,
) -> Result<CompletionRevisionPlan, CompletionPlanError> {
    let expected_production_revision = state.production().revision();
    let expected_equipment_revision = state.equipment().revision();
    let expected_energy_revision = state.energy().revision();
    Ok(CompletionRevisionPlan {
        expected_production_revision,
        next_production_revision: planned_revision(
            expected_production_revision,
            production_changed,
            CompletionPlanError::ProductionRevision,
        )?,
        expected_equipment_revision,
        next_equipment_revision: planned_revision(
            expected_equipment_revision,
            equipment_changed,
            CompletionPlanError::EquipmentRevision,
        )?,
        expected_energy_revision,
        next_energy_revision: planned_revision(
            expected_energy_revision,
            energy_changed,
            CompletionPlanError::EnergyRevision,
        )?,
        expected_structure_revision: state.structures().revision(),
        player_labor_dependencies,
    })
}

fn projected_completion_tick(
    job: &ProductionJobRecord,
    due_ids: &BTreeSet<ProductionJobId>,
    availability_changes: &[ProductionAvailabilityChange],
) -> Option<SimulationTick> {
    if due_ids.contains(&job.id()) {
        return None;
    }
    match find_availability_change(availability_changes, job.id()) {
        Some(ProductionAvailabilityChange::Resumed {
            scheduled_completion,
            ..
        }) => Some(scheduled_completion),
        Some(
            ProductionAvailabilityChange::Suspended { .. }
            | ProductionAvailabilityChange::SuspensionReasonChanged { .. },
        ) => None,
        None if job.is_suspended() => None,
        None => Some(job.completes_at()),
    }
}

fn checked_revision_capacity(revision: u64, steps: impl IntoIterator<Item = u64>) -> bool {
    steps
        .into_iter()
        .try_fold(revision, u64::checked_add)
        .is_some()
}

fn require_revision_capacity(
    revision: u64,
    steps: impl IntoIterator<Item = u64>,
    exhausted: CompletionPlanError,
) -> Result<(), CompletionPlanError> {
    if checked_revision_capacity(revision, steps) {
        Ok(())
    } else {
        Err(exhausted)
    }
}

fn bucket_count(ticks: &BTreeSet<SimulationTick>) -> u64 {
    u64::try_from(ticks.len())
        .unwrap_or_else(|_| unreachable!("projected production bucket count fits memory"))
}

#[derive(Default)]
struct ProjectedRevisionBuckets {
    completion_ticks: BTreeSet<SimulationTick>,
    equipment_ticks: BTreeSet<SimulationTick>,
    energy_ticks: BTreeSet<SimulationTick>,
    structure_ticks: BTreeSet<SimulationTick>,
}

fn project_revision_buckets(
    state: &AppState,
    due_ids: &BTreeSet<ProductionJobId>,
    availability_changes: &[ProductionAvailabilityChange],
) -> ProjectedRevisionBuckets {
    let mut buckets = ProjectedRevisionBuckets::default();
    for job in state.production().jobs() {
        let Some(completes_at) = projected_completion_tick(job, due_ids, availability_changes)
        else {
            continue;
        };
        buckets.completion_ticks.insert(completes_at);
        if job.requires_equipment_revision_at_completion() {
            buckets.equipment_ticks.insert(completes_at);
        }
        if job.requires_energy_revision_at_completion() {
            buckets.energy_ticks.insert(completes_at);
        }
        if job.requires_structure_revision_at_completion(state.inventory()) {
            buckets.structure_ticks.insert(completes_at);
        }
    }
    buckets
}

pub(super) fn validate_resumed_job_revision_capacity(
    state: &AppState,
    due_ids: &BTreeSet<ProductionJobId>,
    plan: &CompletionPlan,
) -> Result<(), CompletionPlanError> {
    let availability_changes = &plan.availability_changes;
    if !availability_changes
        .iter()
        .any(|change| matches!(change, ProductionAvailabilityChange::Resumed { .. }))
    {
        return Ok(());
    }

    let buckets = project_revision_buckets(state, due_ids, availability_changes);
    require_revision_capacity(
        plan.revisions.next_production_revision,
        [bucket_count(&buckets.completion_ticks)],
        CompletionPlanError::ProductionRevision,
    )?;
    require_revision_capacity(
        state.inventory().revision(),
        [
            u64::from(!plan.inventory_deposits.is_empty()),
            state.future_nonproduction_inventory_revision_demand(),
            bucket_count(&buckets.completion_ticks),
        ],
        CompletionPlanError::InventoryRevision,
    )?;
    let nonproduction_equipment_revision_demand = state
        .checked_future_nonproduction_equipment_revision_demand()
        .ok_or(CompletionPlanError::EquipmentRevision)?;
    require_revision_capacity(
        state.equipment().revision(),
        [
            u64::from(!plan.equipment_outcomes.is_empty()),
            nonproduction_equipment_revision_demand,
            bucket_count(&buckets.equipment_ticks),
        ],
        CompletionPlanError::EquipmentRevision,
    )?;
    require_revision_capacity(
        state.energy().revision(),
        [
            u64::from(!plan.released_energy_outcomes.is_empty()),
            state.future_nonproduction_energy_revision_demand(),
            bucket_count(&buckets.energy_ticks),
        ],
        CompletionPlanError::EnergyRevision,
    )?;
    require_revision_capacity(
        state.structures().revision(),
        [
            plan.structural_load
                .as_ref()
                .map_or(0, ValidatedStockpileStructuralLoad::revision_delta),
            state.future_nonproduction_structure_revision_demand(),
            bucket_count(&buckets.structure_ticks),
        ],
        CompletionPlanError::StructuralLoad(StockpileStructuralLoadError::Structure(
            StructuralMutationError::RevisionExhausted,
        )),
    )?;
    Ok(())
}
