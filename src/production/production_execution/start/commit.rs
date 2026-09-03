//! Atomically commits a validated production start.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::state::AppState;
use crate::energy::{
    EnergyStoreOccupancy, apply_prechecked_energy_consumption_reservation, energy_store_occupancy,
};
use crate::equipment::{EquipmentId, EquipmentOccupancy, equipment_occupancy};
use crate::inventory::apply_prechecked_consumption_reservation;
use crate::mining::MiningJobId;
use crate::structural::StructuralCommitError;

use super::ValidatedStartProcess;
use crate::production::{ProductionJobId, ProductionJobRecord};

/// Failure when a validated process start is committed after an owning state has changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartProcessCommitError {
    StaleProductionRevision {
        expected: u64,
        actual: u64,
    },
    StaleInventoryRevision {
        expected: u64,
        actual: u64,
    },
    StaleEnergyRevision {
        expected: u64,
        actual: u64,
    },
    StaleEquipmentRevision {
        expected: u64,
        actual: u64,
    },
    StaleStructureRevision {
        expected: u64,
        actual: u64,
    },
    EnergyStoreBusyManualPower {
        store: crate::energy::EnergyStoreId,
    },
    EquipmentBusyMining {
        equipment: EquipmentId,
        job: MiningJobId,
    },
    EquipmentBusyManualPower {
        equipment: EquipmentId,
    },
    EquipmentBusyProspecting {
        equipment: EquipmentId,
        completes_at: crate::core::time::SimulationTick,
    },
    Structure(StructuralCommitError),
}

impl Display for StartProcessCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleProductionRevision { expected, actual } => write!(
                formatter,
                "validated process start expected production revision {expected} but current revision is {actual}"
            ),
            Self::StaleInventoryRevision { expected, actual } => write!(
                formatter,
                "validated process start expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::StaleEnergyRevision { expected, actual } => write!(
                formatter,
                "validated process start expected energy revision {expected} but current revision is {actual}"
            ),
            Self::StaleEquipmentRevision { expected, actual } => write!(
                formatter,
                "validated process start expected equipment revision {expected} but current revision is {actual}"
            ),
            Self::StaleStructureRevision { expected, actual } => write!(
                formatter,
                "validated process start expected structural revision {expected} but current revision is {actual}"
            ),
            Self::EnergyStoreBusyManualPower { store } => write!(
                formatter,
                "validated process start energy store {} is occupied by direct player-powered generation",
                store.value()
            ),
            Self::EquipmentBusyMining { equipment, job } => write!(
                formatter,
                "validated process start equipment {} is occupied by mining job {}",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyManualPower { equipment } => write!(
                formatter,
                "validated process start equipment {} is occupied by direct player-powered generation",
                equipment.value()
            ),
            Self::EquipmentBusyProspecting {
                equipment,
                completes_at,
            } => write!(
                formatter,
                "validated process start equipment {} is occupied by geological sampling until tick {}",
                equipment.value(),
                completes_at.value()
            ),
            Self::Structure(error) => write!(
                formatter,
                "validated process start could not commit stored-matter structural load: {error}"
            ),
        }
    }
}

impl Error for StartProcessCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Structure(error) => Some(error),
            Self::StaleProductionRevision {
                expected: _expected,
                actual: _actual,
            }
            | Self::StaleInventoryRevision {
                expected: _expected,
                actual: _actual,
            }
            | Self::StaleEnergyRevision {
                expected: _expected,
                actual: _actual,
            }
            | Self::StaleEquipmentRevision {
                expected: _expected,
                actual: _actual,
            }
            | Self::StaleStructureRevision {
                expected: _expected,
                actual: _actual,
            } => None,
            Self::EnergyStoreBusyManualPower { store: _store } => None,
            Self::EquipmentBusyMining {
                equipment: _equipment,
                job: _job,
            } => None,
            Self::EquipmentBusyManualPower {
                equipment: _equipment,
            } => None,
            Self::EquipmentBusyProspecting {
                equipment: _equipment,
                completes_at: _completes_at,
            } => None,
        }
    }
}

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
        Some(EquipmentOccupancy::Production { .. } | EquipmentOccupancy::Maintenance { .. })
        | None => {}
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
