//! Revision-bound mining start commitment and stale-state prechecks.

use crate::core::state::AppState;
use crate::equipment::{EquipmentOccupancy, equipment_occupancy};
use crate::inventory::ValidatedInboundReservation;
use crate::labor::ValidatedPlayerWorkStart;
use crate::mining::{MiningJobId, MiningJobRecord, MiningTargetResolution};

use super::MiningStartRevisions;
use crate::mining::execution::errors::MiningStartCommitError;

#[must_use]
pub struct ValidatedMiningStart {
    target: MiningTargetResolution,
    revisions: MiningStartRevisions,
    next_mining_job_id: u64,
    reservation: ValidatedInboundReservation,
    work: ValidatedPlayerWorkStart,
    record: MiningJobRecord,
}

impl ValidatedMiningStart {
    pub(super) const fn new(
        target: MiningTargetResolution,
        revisions: MiningStartRevisions,
        next_mining_job_id: u64,
        reservation: ValidatedInboundReservation,
        work: ValidatedPlayerWorkStart,
        record: MiningJobRecord,
    ) -> Self {
        Self {
            target,
            revisions,
            next_mining_job_id,
            reservation,
            work,
            record,
        }
    }

    #[cfg(test)]
    pub(in crate::mining::execution) const fn player_work(&self) -> &ValidatedPlayerWorkStart {
        &self.work
    }

    fn precheck_target(&self, state: &AppState) -> Result<(), MiningStartCommitError> {
        if !self.target.still_resolves(state) {
            return Err(MiningStartCommitError::TargetNoLongerResolved);
        }
        let record = state
            .geology()
            .get_deposit(self.record.deposit())
            .unwrap_or_else(|| panic!("re-resolved mining target deposit disappeared"));
        if record.remaining_mass() != self.record.deposit_mass_before() {
            return Err(MiningStartCommitError::TargetChanged);
        }
        Ok(())
    }

    fn precheck_owner_revisions(&self, state: &AppState) -> Result<(), MiningStartCommitError> {
        if state.inventory().revision() != self.reservation.expected_revision() {
            return Err(MiningStartCommitError::StaleInventory {
                expected: self.reservation.expected_revision(),
                actual: state.inventory().revision(),
            });
        }
        if state.equipment().revision() != self.revisions.equipment {
            return Err(MiningStartCommitError::StaleEquipment {
                expected: self.revisions.equipment,
                actual: state.equipment().revision(),
            });
        }
        if state.mining().revision() != self.revisions.mining.expected {
            return Err(MiningStartCommitError::StaleMining {
                expected: self.revisions.mining.expected,
                actual: state.mining().revision(),
            });
        }
        if let Some(expected) = self.revisions.structure
            && state.structures().revision() != expected
        {
            return Err(MiningStartCommitError::StaleStructure {
                expected,
                actual: state.structures().revision(),
            });
        }
        Ok(())
    }

    fn precheck_equipment_occupancy(&self, state: &AppState) -> Result<(), MiningStartCommitError> {
        let equipment = self.record.equipment();
        match equipment_occupancy(state, equipment) {
            Some(EquipmentOccupancy::Production { job, .. }) => {
                return Err(MiningStartCommitError::EquipmentBusyProduction { equipment, job });
            }
            Some(EquipmentOccupancy::Mining { job }) => {
                return Err(MiningStartCommitError::EquipmentBusyMining { equipment, job });
            }
            Some(EquipmentOccupancy::ManualPower { .. }) => {
                return Err(MiningStartCommitError::EquipmentBusyManualPower { equipment });
            }
            Some(
                EquipmentOccupancy::Prospecting { .. } | EquipmentOccupancy::Maintenance { .. },
            )
            | None => {}
        }
        Ok(())
    }

    pub fn commit(self, state: &mut AppState) -> Result<MiningJobId, MiningStartCommitError> {
        self.work
            .precheck(state)
            .map_err(MiningStartCommitError::Work)?;
        self.precheck_target(state)?;
        self.precheck_owner_revisions(state)?;
        self.precheck_equipment_occupancy(state)?;
        self.reservation.assert_matches_state(state.inventory());
        state.mining().assert_job_insertable(
            &self.record,
            self.next_mining_job_id,
            self.revisions.mining.next,
        );
        let id = self.record.id();
        self.reservation.apply(state.inventory_state_mut());
        state.mining_state_mut().insert_job(
            self.record,
            self.next_mining_job_id,
            self.revisions.mining.next,
        );
        self.work.apply(state);
        Ok(id)
    }
}
