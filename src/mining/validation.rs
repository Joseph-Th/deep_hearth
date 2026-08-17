//! Cross-owner persistence validation for mining work-in-progress.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::state::AppState;
use crate::inventory::validate_stockpile_storage;
use crate::registry::Registries;

use super::MiningJobId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MiningReferenceError {
    UnknownMethod { job: MiningJobId },
    UnknownDeposit { job: MiningJobId },
    UnknownDestination { job: MiningJobId },
    UnknownEquipment { job: MiningJobId },
    EquipmentConditionMismatch { job: MiningJobId },
    OutputProfileMismatch { job: MiningJobId },
    OutputStorageInvalid { job: MiningJobId },
    EquipmentAlsoUsedByProduction { job: MiningJobId },
}

impl Display for MiningReferenceError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid mining reference: {self:?}")
    }
}

impl Error for MiningReferenceError {}

pub(crate) fn validate_mining_references(
    registries: &Registries,
    state: &AppState,
) -> Result<(), MiningReferenceError> {
    for job in state.mining().jobs() {
        if registries.mining().get_method(job.method()).is_none() {
            return Err(MiningReferenceError::UnknownMethod { job: job.id() });
        }
        let Some(deposit) = state.geology().get_deposit(job.deposit()) else {
            return Err(MiningReferenceError::UnknownDeposit { job: job.id() });
        };
        let Some(destination) = state.inventory().get_stockpile(job.destination()) else {
            return Err(MiningReferenceError::UnknownDestination { job: job.id() });
        };
        let Some(equipment) = state.equipment().get_equipment(job.equipment()) else {
            return Err(MiningReferenceError::UnknownEquipment { job: job.id() });
        };
        if job.is_working() && equipment.condition() != job.equipment_condition_after() {
            return Err(MiningReferenceError::EquipmentConditionMismatch { job: job.id() });
        }
        let output = job.output();
        if output.commodity() != deposit.commodity()
            || output.temperature() != deposit.temperature()
            || output.composition() != deposit.composition()
            || output.particle_size_distribution().is_some()
        {
            return Err(MiningReferenceError::OutputProfileMismatch { job: job.id() });
        }
        if validate_stockpile_storage(
            registries,
            destination,
            job.destination(),
            output.commodity(),
            output.composition(),
            output.temperature(),
            output.particle_size_distribution(),
        )
        .is_err()
        {
            return Err(MiningReferenceError::OutputStorageInvalid { job: job.id() });
        }
        if job.is_working()
            && state
                .production()
                .get_equipment_occupant(job.equipment())
                .is_some()
        {
            return Err(MiningReferenceError::EquipmentAlsoUsedByProduction { job: job.id() });
        }
    }
    Ok(())
}
