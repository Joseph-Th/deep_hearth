//! In-flight availability, completion planning, and atomic application; sibling start owns admission.

use std::collections::BTreeMap;

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::{SimulationTick, TickSpan};
use crate::energy::{ReleasedEnergyTrace, apply_released_energy_outcomes};
use crate::equipment::EquipmentOperationConditionOutcome;
use crate::inventory::{
    ReservedDepositPlan, ReservedDepositPlanError, ReservedDepositRequest, StockpileId,
    StockpileStoredMassChange, StockpileStructuralLoadError, ValidatedStockpileStructuralLoad,
    apply_reserved_deposits, decide_reserved_deposits, validate_stockpile_stored_mass_changes,
};
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CompletionRevisionPlan {
    expected_production_revision: u64,
    next_production_revision: u64,
    expected_equipment_revision: u64,
    next_equipment_revision: u64,
    expected_energy_revision: u64,
    next_energy_revision: u64,
    expected_structure_revision: u64,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CompletionPlanError {
    MaterialLotIds,
    InventoryRevision,
    ProductionRevision,
    EquipmentRevision,
    EnergyRevision,
    ResumeTickOverflow {
        job: ProductionJobId,
        current: SimulationTick,
        remaining: TickSpan,
    },
    DestinationMassOverflow {
        stockpile: StockpileId,
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

fn decide_availability_changes(
    state: &AppState,
) -> Result<Vec<ProductionAvailabilityChange>, CompletionPlanError> {
    let current = state.tick();
    let mut changes = Vec::new();
    let mut equipment_jobs = state.production().equipment_occupants().collect::<Vec<_>>();
    equipment_jobs.sort_unstable();
    for job_id in equipment_jobs {
        let job = match state.production().get_job(job_id) {
            Some(job) => job,
            None => panic!(
                "runtime invariant broken: equipment occupancy index references missing production job {}",
                job_id.value()
            ),
        };
        if !job.has_required_active_support() {
            continue;
        }
        let provider = match job.equipment_provider() {
            Some(provider) => provider,
            None => panic!(
                "runtime invariant broken: support-dependent production job {} has no equipment provider",
                job.id().value()
            ),
        };
        let support_active = has_required_active_equipment_support(state, job);
        match job.suspension() {
            None if !support_active => {
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
                    reason: ProductionSuspensionReason::EquipmentSupportUnavailable {
                        equipment: provider.equipment(),
                    },
                    suspended_at: current,
                    remaining_active_time: TickSpan::new(remaining),
                });
            }
            Some(suspension) if support_active => {
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
            None | Some(_) => {}
        }
    }
    Ok(changes)
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
    let availability_changes = decide_availability_changes(state)?;
    let mut due_ids = state.production().jobs_due_at(tick);
    for change in &availability_changes {
        match *change {
            ProductionAvailabilityChange::Suspended {
                job,
                reason: _reason,
                suspended_at: _suspended_at,
                remaining_active_time: _remaining_active_time,
            } => {
                due_ids.remove(&job);
            }
            ProductionAvailabilityChange::Resumed {
                job,
                reason: _reason,
                resumed_at: _resumed_at,
                scheduled_completion,
            } if scheduled_completion == tick => {
                due_ids.insert(job);
            }
            ProductionAvailabilityChange::Resumed {
                job: _job,
                reason: _reason,
                resumed_at: _resumed_at,
                scheduled_completion: _scheduled_completion,
            } => {}
        }
    }

    let next_production_revision = if due_ids.is_empty() && availability_changes.is_empty() {
        expected_production_revision
    } else {
        expected_production_revision
            .checked_add(1)
            .ok_or(CompletionPlanError::ProductionRevision)?
    };
    let mut entries = Vec::with_capacity(due_ids.len());
    let mut reserved_deposit_requests = Vec::new();
    let mut equipment_outcomes = Vec::new();
    let mut released_energy_outcomes = Vec::new();
    let mut deposited_mass_by_destination = BTreeMap::<StockpileId, Mass>::new();
    for job_id in &due_ids {
        let job = match state.production().get_job(*job_id) {
            Some(job) => job,
            None => panic!(
                "runtime invariant broken: due index references missing production job {}",
                job_id.value()
            ),
        };
        let mut output_streams = Vec::with_capacity(job.output_streams().len());
        for stream in job.output_streams() {
            let reserved_mass = match sum_lot_spec_mass(stream.outputs()) {
                Some(mass) => mass,
                None => panic!(
                    "runtime invariant broken: production job {} output stream mass overflows",
                    job_id.value()
                ),
            };
            output_streams.push(CompletionOutputStreamPlan {
                id: stream.id(),
                destination: stream.destination(),
            });
            reserved_deposit_requests.push(ReservedDepositRequest::new(
                stream.destination(),
                stream.outputs().to_vec(),
                reserved_mass,
            ));
            let current = deposited_mass_by_destination
                .get(&stream.destination())
                .copied()
                .unwrap_or(Mass::ZERO);
            let next = current.checked_add(reserved_mass).ok_or(
                CompletionPlanError::DestinationMassOverflow {
                    stockpile: stream.destination(),
                },
            )?;
            deposited_mass_by_destination.insert(stream.destination(), next);
        }
        entries.push(CompletionPlanEntry {
            job: *job_id,
            process: job.process(),
            output_streams,
        });
        if let (Some(provider), Some(after)) =
            (job.equipment_provider(), job.equipment_condition_after())
            && after != provider.condition()
        {
            let record = match state.equipment().get_equipment(provider.equipment()) {
                Some(record) => record,
                None => panic!(
                    "runtime invariant broken: production job {} references missing equipment {}",
                    job_id.value(),
                    provider.equipment().value()
                ),
            };
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
            equipment_outcomes.push(EquipmentOperationConditionOutcome::new(
                provider.equipment(),
                provider.condition(),
                after,
            ));
        }
        if let Some(released) = job.released_energy() {
            released_energy_outcomes.push(released);
        }
    }

    let next_equipment_revision = if equipment_outcomes.is_empty() {
        expected_equipment_revision
    } else {
        expected_equipment_revision
            .checked_add(1)
            .ok_or(CompletionPlanError::EquipmentRevision)?
    };
    let next_energy_revision = if released_energy_outcomes.is_empty() {
        expected_energy_revision
    } else {
        expected_energy_revision
            .checked_add(1)
            .ok_or(CompletionPlanError::EnergyRevision)?
    };
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
        mass_changes.push(StockpileStoredMassChange::new_committed_inbound(
            destination,
            stored_after,
        ));
    }
    let structural_load = if mass_changes.is_empty() {
        None
    } else {
        validate_stockpile_stored_mass_changes(registries, state, mass_changes)
            .map_err(CompletionPlanError::StructuralLoad)?
    };
    let inventory_deposits = decide_reserved_deposits(
        state.inventory(),
        tick,
        reserved_deposit_requests,
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
        },
        inventory_deposits,
        availability_changes,
        entries,
        equipment_outcomes,
        released_energy_outcomes,
        structural_load,
    })
}

/// Applies a previously decided due-job plan in stable job-ID order.
pub(crate) fn apply_completion_plan(
    state: &mut AppState,
    plan: CompletionPlan,
) -> Result<CompletionApplication, CompletionCommitError> {
    let CompletionPlan {
        revisions:
            CompletionRevisionPlan {
                expected_production_revision,
                next_production_revision,
                expected_equipment_revision,
                next_equipment_revision,
                expected_energy_revision,
                next_energy_revision,
                expected_structure_revision,
            },
        inventory_deposits,
        availability_changes,
        entries,
        equipment_outcomes,
        released_energy_outcomes,
        structural_load,
    } = plan;

    let expected_inventory_revision = inventory_deposits.expected_revision();
    let actual_inventory_revision = state.inventory().revision();
    if actual_inventory_revision != expected_inventory_revision {
        return Err(CompletionCommitError::InventoryStale {
            expected: expected_inventory_revision,
            actual: actual_inventory_revision,
        });
    }
    if !released_energy_outcomes.is_empty() {
        let actual_energy_revision = state.energy().revision();
        if actual_energy_revision != expected_energy_revision {
            return Err(CompletionCommitError::EnergyRevisionConflict {
                expected: expected_energy_revision,
                actual: actual_energy_revision,
            });
        }
    }
    if !equipment_outcomes.is_empty() || !availability_changes.is_empty() {
        let actual_equipment_revision = state.equipment().revision();
        if actual_equipment_revision != expected_equipment_revision {
            return Err(CompletionCommitError::EquipmentRevisionConflict {
                expected: expected_equipment_revision,
                actual: actual_equipment_revision,
            });
        }
    }
    let actual_production_revision = state.production().revision();
    if actual_production_revision != expected_production_revision {
        return Err(CompletionCommitError::ProductionRevisionChanged {
            expected: expected_production_revision,
            actual: actual_production_revision,
        });
    }
    if structural_load.is_some() || !availability_changes.is_empty() {
        let actual_structure_revision = state.structures().revision();
        if actual_structure_revision != expected_structure_revision {
            return Err(CompletionCommitError::StructureRevisionConflict {
                expected: expected_structure_revision,
                actual: actual_structure_revision,
            });
        }
    }
    if let Some(structural_load) = structural_load {
        debug_assert_eq!(
            structural_load.expected_revision(),
            expected_structure_revision
        );
        structural_load
            .commit(state)
            .map_err(CompletionCommitError::Structure)?;
    }

    for change in &availability_changes {
        match *change {
            ProductionAvailabilityChange::Suspended {
                job,
                reason,
                suspended_at,
                remaining_active_time,
            } => {
                state.production_state_mut().suspend_job(
                    job,
                    suspended_at,
                    remaining_active_time,
                    reason,
                );
            }
            ProductionAvailabilityChange::Resumed {
                job,
                scheduled_completion,
                ..
            } => {
                state
                    .production_state_mut()
                    .resume_job(job, scheduled_completion);
            }
        }
    }

    apply_reserved_deposits(state.inventory_state_mut(), inventory_deposits);

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

    if !completions.is_empty() {
        if !equipment_outcomes.is_empty() {
            state
                .equipment_state_mut()
                .apply_operation_condition_outcomes(
                    expected_equipment_revision,
                    next_equipment_revision,
                    &equipment_outcomes,
                );
        }
        if !released_energy_outcomes.is_empty() {
            apply_released_energy_outcomes(
                state.energy_state_mut(),
                expected_energy_revision,
                next_energy_revision,
                &released_energy_outcomes,
            );
        }
    }
    if !completions.is_empty() || !availability_changes.is_empty() {
        state
            .production_state_mut()
            .apply_revision(next_production_revision);
    }
    Ok(CompletionApplication {
        completions,
        availability_changes,
    })
}
