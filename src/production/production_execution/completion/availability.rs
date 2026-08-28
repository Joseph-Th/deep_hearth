//! Read-only provider and player-labor availability decisions for in-flight production jobs.

use crate::core::state::AppState;
use crate::core::time::{SimulationTick, TickSpan};
use crate::inventory::StockpileId;
use crate::labor::{PlayerWorkTickError, decide_manual_craft_player_work_start};
use crate::registry::Registries;
use crate::structural::StructuralLifecycle;

use super::super::super::state::{ProductionJobRecord, ProductionSuspensionReason};
use super::{CompletionPlanError, PlayerLaborRevisionDependencies, ProductionAvailabilityChange};

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
    if physical_unavailable.is_some() || job.suspension().is_none() {
        return Ok(physical_unavailable);
    }
    if registries.crafting().get_manual(job.process()).is_none() {
        return Ok(None);
    }

    player_labor.player_work_consulted = true;
    if player_labor.claimed {
        return Ok(Some(ProductionSuspensionReason::PlayerLaborUnavailable));
    }
    player_labor.survival_consulted = true;
    let remaining = job
        .suspension()
        .unwrap_or_else(|| {
            panic!("runtime invariant broken: manual resume candidate is not suspended")
        })
        .remaining_active_time();
    match decide_manual_craft_player_work_start(registries, state, job.id(), remaining) {
        Ok(Some(_start)) => {
            player_labor.claimed = true;
            Ok(None)
        }
        Ok(None) => Ok(Some(ProductionSuspensionReason::PlayerLaborUnavailable)),
        Err(PlayerWorkTickError::RevisionExhausted) => Err(CompletionPlanError::PlayerWorkRevision),
    }
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
            Ok(Some(ProductionAvailabilityChange::Suspended {
                job: job.id(),
                reason,
                suspended_at: current,
                remaining_active_time: TickSpan::new(remaining),
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
    ),
    CompletionPlanError,
> {
    let current = state.tick();
    let mut changes = Vec::new();
    let mut player_labor = PlayerLaborAvailabilityState::new(state);
    for job in state.production().jobs() {
        let unavailable = decide_job_unavailability(registries, state, job, &mut player_labor)?;
        if let Some(change) = plan_availability_change(current, job, unavailable)? {
            changes.push(change);
        }
    }
    let player_labor_dependencies = player_labor.revision_dependencies(state);
    Ok((changes, player_labor_dependencies))
}
