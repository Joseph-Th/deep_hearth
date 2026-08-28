//! In-flight availability, completion planning, and atomic application; sibling start owns admission.

use std::collections::BTreeMap;

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::{SimulationTick, TickSpan};
use crate::energy::{ReleasedEnergyTrace, apply_released_energy_outcomes};
use crate::equipment::EquipmentOperationConditionOutcome;
use crate::inventory::{
    AMBIENT_PRESERVATION_MULTIPLIER_PPM, ReservedDepositPlan, ReservedDepositPlanError,
    ReservedDepositRequest, StockpileId, StockpileStoredMassChange, StockpileStructuralLoadError,
    ValidatedStockpileStructuralLoad, apply_reserved_deposits, decide_reserved_deposits,
    validate_stockpile_stored_mass_changes,
};
use crate::labor::{PlayerWorkTickError, decide_manual_craft_player_work_start};
use crate::registry::Registries;
use crate::structural::{StructuralCommitError, StructuralLifecycle};

use super::super::definitions::ProcessId;
use super::super::resolution::{ProcessOutputStreamId, sum_lot_spec_mass};
use super::super::state::{ProductionJobId, ProductionJobRecord, ProductionSuspensionReason};
use super::start::ProcessOutputRoute;

/// Observable active-time scheduling change caused by a production provider becoming unavailable or
/// usable again. Work-in-process remains owned by the same job across both transitions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProductionAvailabilityChange {
    Suspended {
        job: ProductionJobId,
        reason: ProductionSuspensionReason,
        suspended_at: SimulationTick,
        remaining_active_time: TickSpan,
    },
    SuspensionReasonChanged {
        job: ProductionJobId,
        previous: ProductionSuspensionReason,
        reason: ProductionSuspensionReason,
    },
    Resumed {
        job: ProductionJobId,
        reason: ProductionSuspensionReason,
        resumed_at: SimulationTick,
        scheduled_completion: SimulationTick,
    },
}

impl ProductionAvailabilityChange {
    #[must_use]
    pub const fn job(self) -> ProductionJobId {
        match self {
            Self::Suspended {
                job,
                reason: _reason,
                suspended_at: _suspended_at,
                remaining_active_time: _remaining_active_time,
            } => job,
            Self::SuspensionReasonChanged {
                job,
                previous: _previous,
                reason: _reason,
            } => job,
            Self::Resumed {
                job,
                reason: _reason,
                resumed_at: _resumed_at,
                scheduled_completion: _scheduled_completion,
            } => job,
        }
    }
}

/// Observable completion emitted by one simulation tick after authoritative output is committed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessCompletion {
    job: ProductionJobId,
    process: ProcessId,
    routes: Vec<ProcessOutputRoute>,
}

impl ProcessCompletion {
    #[must_use]
    pub const fn job(&self) -> ProductionJobId {
        self.job
    }

    #[must_use]
    pub const fn process(&self) -> ProcessId {
        self.process
    }

    #[must_use]
    pub fn routes(&self) -> &[ProcessOutputRoute] {
        &self.routes
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CompletionPlan {
    revisions: CompletionRevisionPlan,
    inventory_deposits: ReservedDepositPlan,
    availability_changes: Vec<ProductionAvailabilityChange>,
    entries: Vec<CompletionPlanEntry>,
    equipment_outcomes: Vec<EquipmentOperationConditionOutcome>,
    released_energy_outcomes: Vec<ReleasedEnergyTrace>,
    structural_load: Option<ValidatedStockpileStructuralLoad>,
}

impl CompletionPlan {
    pub(crate) fn availability_changes(&self) -> &[ProductionAvailabilityChange] {
        &self.availability_changes
    }

    pub(crate) fn equipment_revision_steps(&self) -> u64 {
        u64::from(!self.equipment_outcomes.is_empty())
    }

    pub(crate) fn energy_revision_steps(&self) -> u64 {
        u64::from(!self.released_energy_outcomes.is_empty())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CompletionRevisionPlan {
    expected_production_revision: u64,
    next_production_revision: u64,
    expected_equipment_revision: u64,
    next_equipment_revision: u64,
    expected_energy_revision: u64,
    next_energy_revision: u64,
    expected_structure_revision: u64,
    player_labor_dependencies: Option<PlayerLaborRevisionDependencies>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PlayerLaborRevisionDependencies {
    expected_player_work_revision: u64,
    expected_survival_revision: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CompletionPlanEntry {
    job: ProductionJobId,
    process: ProcessId,
    output_streams: Vec<CompletionOutputStreamPlan>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CompletionOutputStreamPlan {
    id: ProcessOutputStreamId,
    destination: StockpileId,
}

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CompletionPlanError {
    MaterialLotIds,
    InventoryRevision,
    ProductionRevision,
    EquipmentRevision,
    EnergyRevision,
    PlayerWorkRevision,
    ResumeTickOverflow {
        job: ProductionJobId,
        current: SimulationTick,
        remaining: TickSpan,
    },
    DestinationMassOverflow {
        stockpile: StockpileId,
    },
    StorageAgeOverflow {
        job: ProductionJobId,
    },
    StructuralLoad(StockpileStructuralLoadError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CompletionApplication {
    pub(crate) completions: Vec<ProcessCompletion>,
    pub(crate) availability_changes: Vec<ProductionAvailabilityChange>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CompletionCommitError {
    InventoryStale { expected: u64, actual: u64 },
    ProductionRevisionChanged { expected: u64, actual: u64 },
    EquipmentRevisionConflict { expected: u64, actual: u64 },
    EnergyRevisionConflict { expected: u64, actual: u64 },
    StructureRevisionConflict { expected: u64, actual: u64 },
    PlayerWorkRevisionConflict { expected: u64, actual: u64 },
    SurvivalRevisionConflict { expected: u64, actual: u64 },
    Structure(StructuralCommitError),
}

fn has_required_active_equipment_support(state: &AppState, job: &ProductionJobRecord) -> bool {
    if !job.has_required_active_support() {
        return true;
    }
    let provider = match job.equipment_provider() {
        Some(provider) => provider,
        None => panic!(
            "runtime invariant broken: support-dependent production job {} has no equipment provider",
            job.id().value()
        ),
    };
    let equipment = match state.equipment().get_equipment(provider.equipment()) {
        Some(record) => record,
        None => panic!(
            "runtime invariant broken: production job {} references missing equipment {}",
            job.id().value(),
            provider.equipment().value()
        ),
    };
    equipment.supported_by().is_some_and(|element| {
        state
            .structures()
            .get_element(element)
            .is_some_and(|support| support.lifecycle() == StructuralLifecycle::Active)
    })
}

fn unavailable_output_support(state: &AppState, job: &ProductionJobRecord) -> Option<StockpileId> {
    job.output_streams()
        .iter()
        .map(|stream| stream.destination())
        .find(|destination| {
            let stockpile = state
                .inventory()
                .get_stockpile(*destination)
                .unwrap_or_else(|| {
                    panic!(
                        "runtime invariant broken: production job {} references missing output stockpile {}",
                        job.id().value(),
                        destination.value()
                    )
                });
            stockpile.supported_by().is_some_and(|element| {
                !state
                    .structures()
                    .get_element(element)
                    .is_some_and(|support| support.lifecycle() == StructuralLifecycle::Active)
            })
        })
}

fn current_physical_suspension_reason(
    state: &AppState,
    job: &ProductionJobRecord,
) -> Option<ProductionSuspensionReason> {
    if !has_required_active_equipment_support(state, job) {
        let provider = job.equipment_provider().unwrap_or_else(|| {
            panic!(
                "runtime invariant broken: support-dependent production job {} has no equipment provider",
                job.id().value()
            )
        });
        return Some(ProductionSuspensionReason::EquipmentSupportUnavailable {
            equipment: provider.equipment(),
        });
    }
    unavailable_output_support(state, job)
        .map(|stockpile| ProductionSuspensionReason::OutputSupportUnavailable { stockpile })
}

fn decide_availability_changes(
    registries: &Registries,
    state: &AppState,
) -> Result<
    (
        Vec<ProductionAvailabilityChange>,
        Option<PlayerLaborRevisionDependencies>,
    ),
    CompletionPlanError,
> {
    let current = state.tick();
    let mut changes = Vec::new();
    let mut player_labor_claimed = state.player_work().active().is_some();
    let mut player_work_consulted = false;
    let mut survival_consulted = false;
    for job in state.production().jobs() {
        let physical_unavailable = current_physical_suspension_reason(state, job);
        let unavailable = if physical_unavailable.is_some() || job.suspension().is_none() {
            physical_unavailable
        } else if registries.crafting().get_manual(job.process()).is_some() {
            player_work_consulted = true;
            if player_labor_claimed {
                Some(ProductionSuspensionReason::PlayerLaborUnavailable)
            } else {
                survival_consulted = true;
                let remaining = job
                    .suspension()
                    .unwrap_or_else(|| {
                        panic!("runtime invariant broken: manual resume candidate is not suspended")
                    })
                    .remaining_active_time();
                match decide_manual_craft_player_work_start(registries, state, job.id(), remaining)
                {
                    Ok(Some(_start)) => {
                        player_labor_claimed = true;
                        None
                    }
                    Ok(None) => Some(ProductionSuspensionReason::PlayerLaborUnavailable),
                    Err(PlayerWorkTickError::RevisionExhausted) => {
                        return Err(CompletionPlanError::PlayerWorkRevision);
                    }
                }
            }
        } else {
            None
        };
        match (job.suspension(), unavailable) {
            (None, Some(reason)) => {
                let remaining = job
                    .completes_at()
                    .value()
                    .checked_sub(current.value())
                    .unwrap_or_else(|| {
                        panic!(
                            "runtime invariant broken: running production job {} is already overdue",
                            job.id().value()
                        )
                    });
                assert!(
                    remaining != 0,
                    "runtime invariant broken: running job cannot suspend with zero active time"
                );
                changes.push(ProductionAvailabilityChange::Suspended {
                    job: job.id(),
                    reason,
                    suspended_at: current,
                    remaining_active_time: TickSpan::new(remaining),
                });
            }
            (Some(suspension), None) => {
                let remaining = suspension.remaining_active_time();
                let Some(scheduled_completion) = current.checked_add_span(remaining) else {
                    return Err(CompletionPlanError::ResumeTickOverflow {
                        job: job.id(),
                        current,
                        remaining,
                    });
                };
                changes.push(ProductionAvailabilityChange::Resumed {
                    job: job.id(),
                    reason: suspension.reason(),
                    resumed_at: current,
                    scheduled_completion,
                });
            }
            (Some(suspension), Some(reason)) if suspension.reason() != reason => {
                changes.push(ProductionAvailabilityChange::SuspensionReasonChanged {
                    job: job.id(),
                    previous: suspension.reason(),
                    reason,
                });
            }
            (None, None) | (Some(_), Some(_)) => {}
        }
    }
    let player_labor_dependencies =
        player_work_consulted.then(|| PlayerLaborRevisionDependencies {
            expected_player_work_revision: state.player_work().revision(),
            expected_survival_revision: survival_consulted.then(|| state.survival().revision()),
        });
    Ok((changes, player_labor_dependencies))
}

/// Decides provider availability transitions and all jobs due on one exact tick without mutating
/// production, inventory, equipment, energy, or structure.
pub(crate) fn decide_due_completions(
    registries: &Registries,
    state: &AppState,
    tick: SimulationTick,
) -> Result<CompletionPlan, CompletionPlanError> {
    let expected_production_revision = state.production().revision();
    let expected_equipment_revision = state.equipment().revision();
    let expected_energy_revision = state.energy().revision();
    let expected_structure_revision = state.structures().revision();
    let (availability_changes, player_labor_dependencies) =
        decide_availability_changes(registries, state)?;
    let mut due_ids = state.production().jobs_due_at(tick);
    adjust_due_ids_for_availability(&mut due_ids, &availability_changes, tick);

    let next_production_revision = if due_ids.is_empty() && availability_changes.is_empty() {
        expected_production_revision
    } else {
        expected_production_revision
            .checked_add(1)
            .ok_or(CompletionPlanError::ProductionRevision)?
    };
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

    let next_equipment_revision = if planning.equipment_outcomes.is_empty() {
        expected_equipment_revision
    } else {
        expected_equipment_revision
            .checked_add(1)
            .ok_or(CompletionPlanError::EquipmentRevision)?
    };
    let next_energy_revision = if planning.released_energy_outcomes.is_empty() {
        expected_energy_revision
    } else {
        expected_energy_revision
            .checked_add(1)
            .ok_or(CompletionPlanError::EnergyRevision)?
    };
    let structural_load =
        plan_completion_structural_load(registries, state, planning.deposited_mass_by_destination)?;
    let inventory_deposits = decide_reserved_deposits(
        registries,
        state.inventory(),
        tick,
        planning.deposit_requests,
    )
    .map_err(|error| match error {
        ReservedDepositPlanError::LotIdExhausted => CompletionPlanError::MaterialLotIds,
        ReservedDepositPlanError::RevisionExhausted => CompletionPlanError::InventoryRevision,
    })?;

    Ok(CompletionPlan {
        revisions: CompletionRevisionPlan {
            expected_production_revision,
            next_production_revision,
            expected_equipment_revision,
            next_equipment_revision,
            expected_energy_revision,
            next_energy_revision,
            expected_structure_revision,
            player_labor_dependencies,
        },
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
        });
        planning.deposit_requests.push(ReservedDepositRequest::new(
            stream.destination(),
            stream.outputs().to_vec(),
            reserved_mass,
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

/// Applies a previously decided due-job plan in stable job-ID order.
pub(crate) fn apply_completion_plan(
    state: &mut AppState,
    plan: CompletionPlan,
) -> Result<CompletionApplication, CompletionCommitError> {
    let CompletionPlan {
        revisions,
        inventory_deposits,
        availability_changes,
        entries,
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
    if let Some(structural_load) = structural_load {
        debug_assert_eq!(
            structural_load.expected_revision(),
            revisions.expected_structure_revision
        );
        structural_load
            .commit(state)
            .map_err(CompletionCommitError::Structure)?;
    }

    apply_availability_changes(state, &availability_changes);

    apply_reserved_deposits(state.inventory_state_mut(), inventory_deposits);
    let completions = apply_completion_entries(state, entries);
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
                scheduled_completion,
                ..
            } => state
                .production_state_mut()
                .resume_job(job, scheduled_completion),
        }
    }
}

fn apply_completion_entries(
    state: &mut AppState,
    entries: Vec<CompletionPlanEntry>,
) -> Vec<ProcessCompletion> {
    let mut completions = Vec::with_capacity(entries.len());
    for entry in entries {
        let CompletionPlanEntry {
            job,
            process,
            output_streams,
        } = entry;
        let routes = output_streams
            .iter()
            .map(|stream| ProcessOutputRoute::new(stream.id, stream.destination))
            .collect::<Vec<_>>();
        let removed = state.production_state_mut().remove_job(job);
        debug_assert_eq!(removed.process(), process);
        debug_assert_eq!(removed.output_streams().len(), output_streams.len());
        for (removed_stream, planned_stream) in removed.output_streams().iter().zip(&output_streams)
        {
            debug_assert_eq!(removed_stream.id(), planned_stream.id);
            debug_assert_eq!(removed_stream.destination(), planned_stream.destination);
        }
        completions.push(ProcessCompletion {
            job,
            process,
            routes,
        });
    }
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
