//! Atomic cancellation of unfinished player mining and its reserved output capacity.

use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::equipment::EquipmentId;
use crate::inventory::{
    InboundReservationReleaseError, InventoryState, ValidatedInboundReservationRelease,
    validate_inbound_reservation_release,
};
use crate::maintenance::{Condition, calculate_condition_after_active_ticks};
use crate::registry::Registries;

use super::super::MiningJobId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MiningCancellationError {
    InventoryRevision,
    MiningRevision,
}

#[must_use]
pub(crate) struct MiningCancellationPlan {
    job: MiningJobId,
    equipment: EquipmentId,
    condition_before: Condition,
    condition_after: Condition,
    expected_mining_revision: u64,
    next_mining_revision: u64,
    reservation_release: ValidatedInboundReservationRelease,
}

impl MiningCancellationPlan {
    pub(crate) const fn equipment_revision_steps(&self) -> u64 {
        1
    }
}

pub(crate) fn decide_mining_cancellation(
    registries: &Registries,
    state: &AppState,
    projected_inventory: &InventoryState,
    job: MiningJobId,
    interrupted_at: SimulationTick,
) -> Result<MiningCancellationPlan, MiningCancellationError> {
    let record = state
        .mining()
        .get_job(job)
        .unwrap_or_else(|| panic!("player mining cancellation job disappeared"));
    assert!(record.is_working(), "only working mining can be canceled");
    assert!(
        interrupted_at < record.completes_at(),
        "completed mining must settle through completion rather than cancellation"
    );
    let elapsed = interrupted_at
        .checked_duration_since(record.started_at())
        .unwrap_or_else(|| panic!("runtime invariant broken: mining interruption precedes start"));
    assert!(
        elapsed.value() != 0,
        "mining interruption must settle at least one active tick"
    );
    let method = registries
        .mining()
        .get_method(record.method())
        .unwrap_or_else(|| {
            panic!("runtime invariant broken: interrupted mining method disappeared")
        });
    let condition_before = record.equipment_condition_before();
    let condition_after = calculate_condition_after_active_ticks(
        method.condition_wear_ppm_per_active_tick(),
        condition_before,
        elapsed,
    );
    let reservation_release = validate_inbound_reservation_release(
        projected_inventory,
        record.destination(),
        record.output().mass(),
    )
    .map_err(|error| match error {
        InboundReservationReleaseError::RevisionExhausted => {
            MiningCancellationError::InventoryRevision
        }
    })?;
    let expected_mining_revision = state.mining().revision();
    let next_mining_revision = expected_mining_revision
        .checked_add(1)
        .ok_or(MiningCancellationError::MiningRevision)?;
    Ok(MiningCancellationPlan {
        job,
        equipment: record.equipment(),
        condition_before,
        condition_after,
        expected_mining_revision,
        next_mining_revision,
        reservation_release,
    })
}

pub(crate) fn apply_mining_cancellation(state: &mut AppState, plan: MiningCancellationPlan) {
    let next_equipment_revision = state
        .equipment()
        .revision()
        .checked_add(1)
        .unwrap_or_else(|| panic!("prebudgeted mining interruption revision exhausted"));
    plan.reservation_release
        .assert_matches_state(state.inventory());
    state.mining().assert_working_job_cancellable(
        plan.job,
        plan.expected_mining_revision,
        plan.next_mining_revision,
    );
    state.equipment().assert_condition_change_available(
        plan.equipment,
        plan.condition_before,
        next_equipment_revision,
    );
    plan.reservation_release.apply(state.inventory_state_mut());
    state.equipment_state_mut().apply_condition_change(
        plan.equipment,
        plan.condition_before,
        plan.condition_after,
        next_equipment_revision,
    );
    state.mining_state_mut().cancel_working_job(
        plan.job,
        plan.expected_mining_revision,
        plan.next_mining_revision,
    );
}
