//! Future revision-capacity forecasting for production completion and availability plans.

use crate::core::state::AppState;
use crate::inventory::{StockpileStructuralLoadError, ValidatedStockpileStructuralLoad};
use crate::production::state::ProductionJobId;
use crate::structural::StructuralMutationError;

use super::super::{
    CompletionPlan, CompletionPlanError, CompletionRevisionPlan, PlayerLaborRevisionDependencies,
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

struct RemainingRevisionDemand {
    jobs: u64,
    equipment: u64,
    energy: u64,
    structures: u64,
}

impl RemainingRevisionDemand {
    fn after_completion(state: &AppState, completing: &[ProductionJobId]) -> Self {
        let production = state.production();
        let inventory = state.inventory();
        let mut remaining = Self {
            jobs: production.future_completion_revision_demand(),
            equipment: production.future_equipment_revision_demand(),
            energy: production.future_released_energy_revision_demand(),
            structures: production.future_supported_output_revision_demand(inventory),
        };
        // Completion planning emits a sorted, unique cohort. Subtract only its
        // obligations from the authoritative totals, not one revision per
        // mutation batch: jobs outside this cohort still own their reservations.
        assert!(completing.windows(2).all(|pair| pair[0] < pair[1]));
        for id in completing {
            let job = production
                .get_job(*id)
                .unwrap_or_else(|| unreachable!("planned completion job must exist"));
            remaining.jobs = subtract(remaining.jobs, true);
            remaining.equipment = subtract(
                remaining.equipment,
                job.requires_equipment_revision_at_completion(),
            );
            remaining.energy = subtract(
                remaining.energy,
                job.requires_energy_revision_at_completion(),
            );
            remaining.structures = subtract(
                remaining.structures,
                job.requires_structure_revision_at_completion(inventory),
            );
        }
        remaining
    }
}

fn subtract(total: u64, released: bool) -> u64 {
    total
        .checked_sub(u64::from(released))
        .unwrap_or_else(|| panic!("production completion released an unreserved revision"))
}

/// Prevents the current plan from spending revision capacity retained by jobs that will remain.
pub(super) fn validate_remaining_job_revision_capacity(
    state: &AppState,
    plan: &CompletionPlan,
) -> Result<(), CompletionPlanError> {
    let remaining = RemainingRevisionDemand::after_completion(state, &plan.jobs);
    require_revision_capacity(
        plan.revisions.next_production_revision,
        [remaining.jobs],
        CompletionPlanError::ProductionRevision,
    )?;
    require_revision_capacity(
        state.inventory().revision(),
        [
            u64::from(!plan.inventory_deposits.is_empty()),
            state.future_nonproduction_inventory_revision_demand(),
            remaining.jobs,
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
            remaining.equipment,
        ],
        CompletionPlanError::EquipmentRevision,
    )?;
    require_revision_capacity(
        state.energy().revision(),
        [
            u64::from(!plan.released_energy_outcomes.is_empty()),
            state.future_nonproduction_energy_revision_demand(),
            remaining.energy,
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
            remaining.structures,
        ],
        CompletionPlanError::StructuralLoad(StockpileStructuralLoadError::Structure(
            StructuralMutationError::RevisionExhausted,
        )),
    )?;
    Ok(())
}
