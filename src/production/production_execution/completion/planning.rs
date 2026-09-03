//! Read-only planning for due production jobs and their crossed-owner completion effects.

use std::collections::BTreeMap;

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::energy::ReleasedEnergyTrace;
use crate::equipment::EquipmentOperationConditionOutcome;
use crate::inventory::{
    AMBIENT_PRESERVATION_MULTIPLIER_PPM, ReservedDepositPlanError, ReservedDepositRequest,
    StockpileId, StockpileStoredMassChange, ValidatedStockpileStructuralLoad,
    decide_reserved_deposits, validate_stockpile_stored_mass_changes,
};
use crate::registry::Registries;

use super::super::super::resolution::sum_lot_spec_mass;
use super::super::super::state::{ProductionJobId, ProductionJobRecord};
use super::availability::decide_availability_changes;
use super::{
    CompletionOutputStreamPlan, CompletionPlan, CompletionPlanEntry, CompletionPlanError,
    CompletionRevisionPlan, PlayerLaborRevisionDependencies, ProductionAvailabilityChange,
};

struct DueCompletionPlanning {
    entries: Vec<CompletionPlanEntry>,
    deposit_requests: Vec<ReservedDepositRequest>,
    equipment_outcomes: Vec<EquipmentOperationConditionOutcome>,
    released_energy_outcomes: Vec<ReleasedEnergyTrace>,
    deposited_mass_by_destination: BTreeMap<StockpileId, Mass>,
}

impl DueCompletionPlanning {
    fn new(job_count: usize) -> Self {
        Self {
            entries: Vec::with_capacity(job_count),
            deposit_requests: Vec::new(),
            equipment_outcomes: Vec::new(),
            released_energy_outcomes: Vec::new(),
            deposited_mass_by_destination: BTreeMap::new(),
        }
    }

    fn add_deposited_mass(
        &mut self,
        destination: StockpileId,
        mass: Mass,
    ) -> Result<(), CompletionPlanError> {
        let current = self
            .deposited_mass_by_destination
            .get(&destination)
            .copied()
            .unwrap_or(Mass::ZERO);
        let next =
            current
                .checked_add(mass)
                .ok_or(CompletionPlanError::DestinationMassOverflow {
                    stockpile: destination,
                })?;
        self.deposited_mass_by_destination.insert(destination, next);
        Ok(())
    }
}

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

fn build_completion_revision_plan(
    state: &AppState,
    production_changed: bool,
    planning: &DueCompletionPlanning,
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
            !planning.equipment_outcomes.is_empty(),
            CompletionPlanError::EquipmentRevision,
        )?,
        expected_energy_revision,
        next_energy_revision: planned_revision(
            expected_energy_revision,
            !planning.released_energy_outcomes.is_empty(),
            CompletionPlanError::EnergyRevision,
        )?,
        expected_structure_revision: state.structures().revision(),
        player_labor_dependencies,
    })
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
        plan_due_job(state, tick, job, &mut planning)?;
    }
    let revisions = build_completion_revision_plan(
        state,
        !due_ids.is_empty() || !availability_changes.is_empty(),
        &planning,
        player_labor_dependencies,
    )?;
    let structural_load =
        plan_completion_structural_load(registries, state, planning.deposited_mass_by_destination)?;
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

    Ok(CompletionPlan {
        revisions,
        inventory_deposits,
        availability_changes,
        entries: planning.entries,
        equipment_outcomes: planning.equipment_outcomes,
        released_energy_outcomes: planning.released_energy_outcomes,
        structural_load,
    })
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
) -> Result<(), CompletionPlanError> {
    let storage_age_parts = job
        .material_storage_history()
        .project(tick, AMBIENT_PRESERVATION_MULTIPLIER_PPM)
        .ok_or(CompletionPlanError::StorageAgeOverflow { job: job.id() })?;
    let output_streams = plan_due_job_outputs(job, storage_age_parts, planning)?;
    planning.entries.push(CompletionPlanEntry {
        job: job.id(),
        process: job.process(),
        output_streams,
    });
    plan_due_job_equipment(state, job, &mut planning.equipment_outcomes);
    if let Some(released) = job.released_energy() {
        planning.released_energy_outcomes.push(released);
    }
    Ok(())
}

fn plan_due_job_outputs(
    job: &ProductionJobRecord,
    storage_age_parts: u128,
    planning: &mut DueCompletionPlanning,
) -> Result<Vec<CompletionOutputStreamPlan>, CompletionPlanError> {
    let mut output_streams = Vec::with_capacity(job.output_streams().len());
    for stream in job.output_streams() {
        let reserved_mass = sum_lot_spec_mass(stream.outputs()).unwrap_or_else(|| {
            panic!(
                "runtime invariant broken: production job {} output stream mass overflows",
                job.id().value()
            )
        });
        output_streams.push(CompletionOutputStreamPlan {
            id: stream.id(),
            destination: stream.destination(),
            outputs: stream.outputs().to_vec(),
        });
        planning.deposit_requests.push(ReservedDepositRequest::new(
            stream.destination(),
            stream.outputs().to_vec(),
            storage_age_parts,
        ));
        planning.add_deposited_mass(stream.destination(), reserved_mass)?;
    }
    Ok(output_streams)
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
    deposited_mass_by_destination: BTreeMap<StockpileId, Mass>,
) -> Result<Option<ValidatedStockpileStructuralLoad>, CompletionPlanError> {
    let mut mass_changes = Vec::with_capacity(deposited_mass_by_destination.len());
    for (destination, deposited) in deposited_mass_by_destination {
        let record = state
            .inventory()
            .get_stockpile(destination)
            .unwrap_or_else(|| {
                panic!(
                    "due production destination {} disappeared",
                    destination.value()
                )
            });
        let stored_after = record.stored_mass().checked_add(deposited).ok_or(
            CompletionPlanError::DestinationMassOverflow {
                stockpile: destination,
            },
        )?;
        mass_changes.push(StockpileStoredMassChange::new(destination, stored_after));
    }
    if mass_changes.is_empty() {
        return Ok(None);
    }
    validate_stockpile_stored_mass_changes(registries, state, mass_changes)
        .map_err(CompletionPlanError::StructuralLoad)
}
