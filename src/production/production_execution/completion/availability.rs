//! Read-only provider and player-labor availability decisions for in-flight production jobs.

use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::equipment::EquipmentId;
use crate::inventory::StockpileId;
use crate::labor::{PlayerWorkTickError, decide_manual_production_player_work_start};
use crate::registry::Registries;
use crate::structural::StructuralLifecycle;

use super::super::super::state::{
    ProductionAvailabilityDependencyRevisions, ProductionJobId, ProductionJobRecord,
    ProductionSuspensionReason,
};
use super::{CompletionPlanError, PlayerLaborRevisionDependencies, ProductionAvailabilityChange};

fn unavailable_equipment_support(
    state: &AppState,
    job: &ProductionJobRecord,
) -> Option<EquipmentId> {
    if !job.has_required_active_support() {
        return None;
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
    let support_available = equipment.supported_by().is_some_and(|element| {
        state
            .structures()
            .get_element(element)
            .is_some_and(|support| support.lifecycle() == StructuralLifecycle::Active)
    });
    (!support_available).then_some(provider.equipment())
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
    if let Some(equipment) = unavailable_equipment_support(state, job) {
        return Some(ProductionSuspensionReason::EquipmentSupportUnavailable { equipment });
    }
    unavailable_output_support(state, job)
        .map(|stockpile| ProductionSuspensionReason::OutputSupportUnavailable { stockpile })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PlayerLaborAvailabilityState {
    claimed: bool,
    player_work_consulted: bool,
    survival_consulted: bool,
}

impl PlayerLaborAvailabilityState {
    fn new(state: &AppState) -> Self {
        Self {
            claimed: state.player_work().active().is_some(),
            player_work_consulted: false,
            survival_consulted: false,
        }
    }

    fn revision_dependencies(self, state: &AppState) -> Option<PlayerLaborRevisionDependencies> {
        self.player_work_consulted
            .then(|| PlayerLaborRevisionDependencies {
                expected_player_work_revision: state.player_work().revision(),
                expected_survival_revision: self
                    .survival_consulted
                    .then(|| state.survival().revision()),
            })
    }
}

fn decide_job_unavailability(
    registries: &Registries,
    state: &AppState,
    job: &ProductionJobRecord,
    player_labor: &mut PlayerLaborAvailabilityState,
) -> Result<Option<ProductionSuspensionReason>, CompletionPlanError> {
    let physical_unavailable = current_physical_suspension_reason(state, job);
    if physical_unavailable.is_some() {
        return Ok(physical_unavailable);
    }
    let Some(suspension) = job.suspension() else {
        return Ok(None);
    };
    if registries.manual_process_exertion(job.process()).is_none() {
        return Ok(None);
    }

    player_labor.player_work_consulted = true;
    if player_labor.claimed {
        return Ok(Some(ProductionSuspensionReason::PlayerLaborUnavailable));
    }
    player_labor.survival_consulted = true;
    let remaining = suspension.remaining_active_time();
    match decide_manual_production_player_work_start(registries, state, job.id(), remaining) {
        Ok(Some(_start)) => {
            player_labor.claimed = true;
            Ok(None)
        }
        Ok(None) => Ok(Some(ProductionSuspensionReason::PlayerLaborUnavailable)),
        Err(PlayerWorkTickError::RevisionExhausted) => Err(CompletionPlanError::PlayerWorkRevision),
    }
}

fn indexed_job<'state>(
    state: &'state AppState,
    job_id: ProductionJobId,
    index_name: &str,
) -> &'state ProductionJobRecord {
    state.production().get_job(job_id).unwrap_or_else(|| {
        panic!(
            "runtime invariant broken: {index_name} references missing production job {}",
            job_id.value()
        )
    })
}

fn evaluate_availability_candidates(
    registries: &Registries,
    state: &AppState,
    candidates: impl IntoIterator<Item = ProductionJobId>,
    index_name: &str,
    stop_after_resume: bool,
    player_labor: &mut PlayerLaborAvailabilityState,
    changes: &mut Vec<ProductionAvailabilityChange>,
) -> Result<(), CompletionPlanError> {
    let current = state.tick();
    for job_id in candidates {
        let job = indexed_job(state, job_id, index_name);
        let unavailable = decide_job_unavailability(registries, state, job, player_labor)?;
        if let Some(change) = plan_availability_change(current, job, unavailable)? {
            let resumed = matches!(change, ProductionAvailabilityChange::Resumed { .. });
            changes.push(change);
            if stop_after_resume && resumed {
                // Player attention is exclusive. Once the lowest-ID feasible suspended job claims
                // it, every later player-labor-suspended job remains blocked until a later tick.
                break;
            }
        }
    }
    Ok(())
}

fn plan_availability_change(
    current: SimulationTick,
    job: &ProductionJobRecord,
    unavailable: Option<ProductionSuspensionReason>,
) -> Result<Option<ProductionAvailabilityChange>, CompletionPlanError> {
    match (job.suspension(), unavailable) {
        (None, Some(reason)) => {
            let remaining = job
                .completes_at()
                .checked_duration_since(current)
                .unwrap_or_else(|| {
                    panic!(
                        "runtime invariant broken: running production job {} is already overdue",
                        job.id().value()
                    )
                });
            assert!(
                !remaining.is_zero(),
                "runtime invariant broken: running job cannot suspend with zero active time"
            );
            Ok(Some(ProductionAvailabilityChange::Suspended {
                job: job.id(),
                reason,
                suspended_at: current,
                remaining_active_time: remaining,
            }))
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
            Ok(Some(ProductionAvailabilityChange::Resumed {
                job: job.id(),
                reason: suspension.reason(),
                resumed_at: current,
                scheduled_completion,
            }))
        }
        (Some(suspension), Some(reason)) if suspension.reason() != reason => Ok(Some(
            ProductionAvailabilityChange::SuspensionReasonChanged {
                job: job.id(),
                previous: suspension.reason(),
                reason,
            },
        )),
        (None, None) | (Some(_), Some(_)) => Ok(None),
    }
}

pub(super) fn decide_availability_changes(
    registries: &Registries,
    state: &AppState,
) -> Result<
    (
        Vec<ProductionAvailabilityChange>,
        Option<PlayerLaborRevisionDependencies>,
        ProductionAvailabilityDependencyRevisions,
    ),
    CompletionPlanError,
> {
    let dependency_revisions = ProductionAvailabilityDependencyRevisions::new(
        state.inventory().support_revision(),
        state.equipment().support_revision(),
        state.structures().revision(),
    );
    if state.production().is_empty() {
        return Ok((Vec::new(), None, dependency_revisions));
    }
    let mut changes = Vec::new();
    let mut player_labor = PlayerLaborAvailabilityState::new(state);
    if state
        .production()
        .physical_availability_dependencies_changed(dependency_revisions)
    {
        evaluate_availability_candidates(
            registries,
            state,
            state
                .production()
                .physical_availability_candidate_jobs(state.inventory()),
            "availability candidate index",
            false,
            &mut player_labor,
            &mut changes,
        )?;
    } else if !player_labor.claimed {
        evaluate_availability_candidates(
            registries,
            state,
            state.production().player_labor_suspended_jobs(),
            "player-labor suspension index",
            true,
            &mut player_labor,
            &mut changes,
        )?;
    }
    let player_labor_dependencies = player_labor.revision_dependencies(state);
    Ok((changes, player_labor_dependencies, dependency_revisions))
}
