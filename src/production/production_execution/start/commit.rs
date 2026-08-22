//! Atomic commit for a previously validated production start.

use crate::core::state::AppState;
use crate::energy::{EnergyCommitError, apply_energy_consumption_reservation};
use crate::inventory::{ReservationCommitError, apply_consumption_reservation};

use super::{StartProcessCommitError, ValidatedStartProcess};
use crate::production::ProductionJobId;

impl ValidatedStartProcess {
    pub(crate) const fn job_id(&self) -> ProductionJobId {
        self.job.identity.id
    }

    /// Commits input consumption, output reservation, and job insertion as one canonical operation.
    pub fn commit(self, state: &mut AppState) -> Result<ProductionJobId, StartProcessCommitError> {
        let Self {
            job,
            next_job_id,
            expected_production_revision,
            next_production_revision,
            reservation,
            energy_reservation,
            energy_ingress_reservation,
            equipment_use,
            destination_structure_revision,
            structural_load,
        } = self;
        let job_id = job.id();

        for store in job
            .consumed_energy()
            .map(|trace| trace.source())
            .into_iter()
            .chain(job.released_energy().map(|trace| trace.destination()))
        {
            if state
                .player_work()
                .get_manual_power_energy_occupant(store)
                .is_some()
            {
                return Err(StartProcessCommitError::EnergyStoreBusyManualPower { store });
            }
        }
        if let Some(provider) = job.equipment_provider() {
            let equipment = provider.equipment();
            if let Some(mining_job) = state.mining().get_equipment_occupant(equipment) {
                return Err(StartProcessCommitError::EquipmentBusyMining {
                    equipment,
                    job: mining_job,
                });
            }
            if state
                .player_work()
                .get_manual_power_equipment_occupant(equipment)
                .is_some()
            {
                return Err(StartProcessCommitError::EquipmentBusyManualPower { equipment });
            }
        }

        let actual_production_revision = state.production().revision();
        if actual_production_revision != expected_production_revision {
            return Err(StartProcessCommitError::StaleProductionRevision {
                expected: expected_production_revision,
                actual: actual_production_revision,
            });
        }
        if let Some(energy) = energy_ingress_reservation {
            let expected_energy_revision = energy.expected_revision();
            let actual_energy_revision = state.energy().revision();
            if actual_energy_revision != expected_energy_revision {
                return Err(StartProcessCommitError::StaleEnergyRevision {
                    expected: expected_energy_revision,
                    actual: actual_energy_revision,
                });
            }
        }
        let expected_inventory_revision = reservation.expected_revision();
        let actual_inventory_revision = state.inventory().revision();
        if actual_inventory_revision != expected_inventory_revision {
            return Err(StartProcessCommitError::StaleInventoryRevision {
                expected: expected_inventory_revision,
                actual: actual_inventory_revision,
            });
        }
        if let Some(energy) = energy_reservation {
            let expected_energy_revision = energy.expected_revision();
            let actual_energy_revision = state.energy().revision();
            if actual_energy_revision != expected_energy_revision {
                return Err(StartProcessCommitError::StaleEnergyRevision {
                    expected: expected_energy_revision,
                    actual: actual_energy_revision,
                });
            }
        }
        if let Some(equipment) = equipment_use {
            let expected_equipment_revision = equipment.expected_equipment_revision();
            let actual_equipment_revision = state.equipment().revision();
            if actual_equipment_revision != expected_equipment_revision {
                return Err(StartProcessCommitError::StaleEquipmentRevision {
                    expected: expected_equipment_revision,
                    actual: actual_equipment_revision,
                });
            }
            if let Some(expected_structure_revision) = equipment.expected_structure_revision() {
                let actual_structure_revision = state.structures().revision();
                if actual_structure_revision != expected_structure_revision {
                    return Err(StartProcessCommitError::StaleStructureRevision {
                        expected: expected_structure_revision,
                        actual: actual_structure_revision,
                    });
                }
            }
        }
        if let Some(expected_structure_revision) = destination_structure_revision {
            let actual_structure_revision = state.structures().revision();
            if actual_structure_revision != expected_structure_revision {
                return Err(StartProcessCommitError::StaleStructureRevision {
                    expected: expected_structure_revision,
                    actual: actual_structure_revision,
                });
            }
        }
        if let Some(structural_load) = &structural_load {
            let expected_structure_revision = structural_load.expected_revision();
            let actual_structure_revision = state.structures().revision();
            if actual_structure_revision != expected_structure_revision {
                return Err(StartProcessCommitError::StaleStructureRevision {
                    expected: expected_structure_revision,
                    actual: actual_structure_revision,
                });
            }
        }
        if let Some(structural_load) = structural_load {
            structural_load
                .commit(state)
                .map_err(StartProcessCommitError::Structure)?;
        }
        apply_consumption_reservation(state.inventory_state_mut(), reservation).map_err(
            |error| match error {
                ReservationCommitError::StaleInventoryRevision { expected, actual } => {
                    StartProcessCommitError::StaleInventoryRevision { expected, actual }
                }
            },
        )?;
        if let Some(energy) = energy_reservation {
            apply_energy_consumption_reservation(state.energy_state_mut(), energy).map_err(
                |error| match error {
                    EnergyCommitError::StaleRevision { expected, actual } => {
                        StartProcessCommitError::StaleEnergyRevision { expected, actual }
                    }
                },
            )?;
        }
        state
            .production_state_mut()
            .insert_job(job, next_job_id, next_production_revision);
        Ok(job_id)
    }
}
