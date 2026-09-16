//! Atomic cancellation of unfinished player mining and its reserved output capacity.

use crate::core::state::AppState;
use crate::inventory::{
    InboundReservationReleaseError, InventoryState, ValidatedInboundReservationRelease,
    validate_inbound_reservation_release,
};

use super::super::MiningJobId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MiningCancellationError {
    InventoryRevision,
    MiningRevision,
}

#[must_use]
pub(crate) struct MiningCancellationPlan {
    job: MiningJobId,
    expected_mining_revision: u64,
    next_mining_revision: u64,
    reservation_release: ValidatedInboundReservationRelease,
}

pub(crate) fn decide_mining_cancellation(
    state: &AppState,
    projected_inventory: &InventoryState,
    job: MiningJobId,
) -> Result<MiningCancellationPlan, MiningCancellationError> {
    let record = state
        .mining()
        .get_job(job)
        .unwrap_or_else(|| panic!("player mining cancellation job disappeared"));
    assert!(record.is_working(), "only working mining can be canceled");
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
        expected_mining_revision,
        next_mining_revision,
        reservation_release,
    })
}

pub(crate) fn apply_mining_cancellation(state: &mut AppState, plan: MiningCancellationPlan) {
    plan.reservation_release
        .assert_matches_state(state.inventory());
    state.mining().assert_working_job_cancellable(
        plan.job,
        plan.expected_mining_revision,
        plan.next_mining_revision,
    );
    plan.reservation_release.apply(state.inventory_state_mut());
    state.mining_state_mut().cancel_working_job(
        plan.job,
        plan.expected_mining_revision,
        plan.next_mining_revision,
    );
}
