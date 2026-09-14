//! Atomically commits a validated production start.

use crate::core::state::AppState;
use crate::energy::{
    EnergyStoreOccupancy, apply_prechecked_energy_consumption_reservation, energy_store_occupancy,
};
use crate::equipment::{EquipmentOccupancy, equipment_occupancy};
use crate::inventory::apply_prechecked_consumption_reservation;

use super::ValidatedStartProcess;
use crate::production::{ProductionJobId, ProductionJobRecord};

mod errors;

pub use errors::StartProcessCommitError;

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

        validate_commit_occupancy(state, &job)?;
        validate_production_revision(state, expected_production_revision)?;
        if let Some(energy) = &energy_ingress_reservation {
            validate_energy_revision(state, energy.expected_revision())?;
        }
        validate_inventory_revision(state, reservation.expected_revision())?;
        if let Some(energy) = &energy_reservation {
            validate_energy_revision(state, energy.expected_revision())?;
        }
        if let Some(equipment) = equipment_use {
            validate_equipment_revision(state, equipment.expected_equipment_revision())?;
            if let Some(expected_structure_revision) = equipment.expected_structure_revision() {
                validate_structure_revision(state, expected_structure_revision)?;
            }
        }
        if let Some(expected_structure_revision) = destination_structure_revision {
            validate_structure_revision(state, expected_structure_revision)?;
        }
        if let Some(structural_load) = &structural_load {
            validate_structure_revision(state, structural_load.expected_revision())?;
        }
        reservation.assert_matches_state(state.inventory());
        if let Some(energy) = &energy_reservation {
            energy.assert_matches_state(state.energy());
        }
        if let Some(energy) = &energy_ingress_reservation {
            energy.assert_matches_state(state.energy());
        }
        state
            .production()
            .assert_job_insertable(&job, next_job_id, next_production_revision);
        if let Some(structural_load) = structural_load {
            structural_load
                .commit(state)
                .map_err(StartProcessCommitError::Structure)?;
        }
        apply_prechecked_consumption_reservation(state.inventory_state_mut(), reservation);
        if let Some(energy) = energy_reservation {
            let _ =
                apply_prechecked_energy_consumption_reservation(state.energy_state_mut(), energy);
        }
        state
            .production_state_mut()
            .insert_job(job, next_job_id, next_production_revision);
        Ok(job_id)
    }
}

fn validate_commit_occupancy(
    state: &AppState,
    job: &ProductionJobRecord,
) -> Result<(), StartProcessCommitError> {
    for store in job
        .consumed_energy()
        .map(|trace| trace.source())
        .into_iter()
        .chain(job.released_energy().map(|trace| trace.destination()))
    {
        if matches!(
            energy_store_occupancy(state, store),
            Some(EnergyStoreOccupancy::ManualPower)
        ) {
            return Err(StartProcessCommitError::EnergyStoreBusyManualPower { store });
        }
    }
    let Some(provider) = job.equipment_provider() else {
        return Ok(());
    };
    let equipment = provider.equipment();
    match equipment_occupancy(state, equipment) {
        Some(EquipmentOccupancy::Mining { job }) => {
            return Err(StartProcessCommitError::EquipmentBusyMining { equipment, job });
        }
        Some(EquipmentOccupancy::ManualPower { .. }) => {
            return Err(StartProcessCommitError::EquipmentBusyManualPower { equipment });
        }
        Some(EquipmentOccupancy::Prospecting { completes_at }) => {
            return Err(StartProcessCommitError::EquipmentBusyProspecting {
                equipment,
                completes_at,
            });
        }
        Some(EquipmentOccupancy::Maintenance { completes_at }) => {
            return Err(StartProcessCommitError::EquipmentUnderMaintenance {
                equipment,
                completes_at,
            });
        }
        Some(EquipmentOccupancy::Production { .. }) | None => {}
    }
    Ok(())
}

fn validate_production_revision(
    state: &AppState,
    expected: u64,
) -> Result<(), StartProcessCommitError> {
    let actual = state.production().revision();
    if actual != expected {
        return Err(StartProcessCommitError::StaleProductionRevision { expected, actual });
    }
    Ok(())
}

fn validate_inventory_revision(
    state: &AppState,
    expected: u64,
) -> Result<(), StartProcessCommitError> {
    let actual = state.inventory().revision();
    if actual != expected {
        return Err(StartProcessCommitError::StaleInventoryRevision { expected, actual });
    }
    Ok(())
}

fn validate_energy_revision(
    state: &AppState,
    expected: u64,
) -> Result<(), StartProcessCommitError> {
    let actual = state.energy().revision();
    if actual != expected {
        return Err(StartProcessCommitError::StaleEnergyRevision { expected, actual });
    }
    Ok(())
}

fn validate_equipment_revision(
    state: &AppState,
    expected: u64,
) -> Result<(), StartProcessCommitError> {
    let actual = state.equipment().revision();
    if actual != expected {
        return Err(StartProcessCommitError::StaleEquipmentRevision { expected, actual });
    }
    Ok(())
}

fn validate_structure_revision(
    state: &AppState,
    expected: u64,
) -> Result<(), StartProcessCommitError> {
    let actual = state.structures().revision();
    if actual != expected {
        return Err(StartProcessCommitError::StaleStructureRevision { expected, actual });
    }
    Ok(())
}
