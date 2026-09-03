//! Commit-stage stale-state checks and atomic equipment-maintenance mutation.

use crate::core::state::AppState;
use crate::equipment::{EquipmentOccupancy, equipment_occupancy};

use super::{
    EquipmentMaintenanceCommitError, EquipmentMaintenanceStartOutcome,
    ValidatedEquipmentMaintenance,
};

impl ValidatedEquipmentMaintenance {
    pub fn commit(
        self,
        state: &mut AppState,
    ) -> Result<EquipmentMaintenanceStartOutcome, EquipmentMaintenanceCommitError> {
        let actual_revision = state.equipment().revision();
        if actual_revision != self.expected_equipment_revision {
            return Err(EquipmentMaintenanceCommitError::StaleEquipmentRevision {
                expected: self.expected_equipment_revision,
                actual: actual_revision,
            });
        }
        match equipment_occupancy(state, self.equipment) {
            Some(EquipmentOccupancy::Production { job, release }) => {
                return Err(EquipmentMaintenanceCommitError::EquipmentBusy {
                    equipment: self.equipment,
                    job,
                    release,
                });
            }
            Some(EquipmentOccupancy::Mining { job }) => {
                return Err(EquipmentMaintenanceCommitError::EquipmentBusyMining {
                    equipment: self.equipment,
                    job,
                });
            }
            Some(EquipmentOccupancy::ManualPower { .. }) => {
                return Err(EquipmentMaintenanceCommitError::EquipmentBusyManualPower {
                    equipment: self.equipment,
                });
            }
            Some(EquipmentOccupancy::Prospecting { completes_at }) => {
                return Err(EquipmentMaintenanceCommitError::EquipmentBusyProspecting {
                    equipment: self.equipment,
                    completes_at,
                });
            }
            Some(EquipmentOccupancy::Maintenance { .. }) | None => {}
        }
        let Some(record) = state.equipment().get_equipment(self.equipment) else {
            return Err(EquipmentMaintenanceCommitError::UnknownEquipment {
                equipment: self.equipment,
            });
        };
        if record.condition() != self.condition_before {
            return Err(EquipmentMaintenanceCommitError::ConditionChanged {
                equipment: self.equipment,
                expected: self.condition_before,
                actual: record.condition(),
            });
        }
        self.player_work
            .precheck(state)
            .map_err(EquipmentMaintenanceCommitError::PlayerWork)?;

        let material_mass = self.material.material_mass();
        self.material.commit(
            state,
            self.equipment,
            self.condition_before,
            self.expected_equipment_revision,
            self.next_equipment_revision,
        )?;
        self.player_work.apply(state);

        Ok(EquipmentMaintenanceStartOutcome {
            equipment: self.equipment,
            condition_before: self.condition_before,
            target_condition: self.condition_after,
            material_mass,
            completes_at: self.work.completes_at(),
        })
    }
}
