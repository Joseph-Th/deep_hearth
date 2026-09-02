//! Internal subsystem failures translated into the public canonical tick failure surface.

use crate::geology::FieldProspectingTickError;
use crate::inventory::StorageEnclosureDismantlingTickError;
use crate::labor::ManualPowerTickError;
use crate::mining::MiningTickError;
use crate::production::{CompletionCommitError, CompletionPlanError};
use crate::survival::SurvivalTickError;

use super::TickError;

impl From<CompletionPlanError> for TickError {
    fn from(error: CompletionPlanError) -> Self {
        match error {
            CompletionPlanError::MaterialLotIds => Self::MaterialLotIdExhausted,
            CompletionPlanError::InventoryRevision => Self::InventoryRevisionExhausted,
            CompletionPlanError::ProductionRevision => Self::ProductionRevisionExhausted,
            CompletionPlanError::EquipmentRevision => Self::EquipmentRevisionExhausted,
            CompletionPlanError::EnergyRevision => Self::EnergyRevisionExhausted,
            CompletionPlanError::PlayerWorkRevision => Self::PlayerWorkRevisionExhausted,
            CompletionPlanError::ResumeTickOverflow {
                job,
                current,
                remaining,
            } => Self::ProductionResumeTickOverflow {
                job,
                current,
                remaining,
            },
            CompletionPlanError::DestinationMassOverflow { stockpile } => {
                Self::DestinationMassOverflow { stockpile }
            }
            CompletionPlanError::StorageAgeOverflow { job } => {
                Self::ProductionStorageAgeOverflow { job }
            }
            CompletionPlanError::StructuralLoad(error) => Self::StructuralLoad(error),
        }
    }
}

impl From<CompletionCommitError> for TickError {
    fn from(error: CompletionCommitError) -> Self {
        match error {
            CompletionCommitError::InventoryStale { expected, actual } => {
                Self::StaleInventoryRevision { expected, actual }
            }
            CompletionCommitError::ProductionRevisionChanged { expected, actual } => {
                Self::StaleProductionRevision { expected, actual }
            }
            CompletionCommitError::EquipmentRevisionConflict { expected, actual } => {
                Self::StaleEquipmentRevision { expected, actual }
            }
            CompletionCommitError::EnergyRevisionConflict { expected, actual } => {
                Self::StaleEnergyRevision { expected, actual }
            }
            CompletionCommitError::StructureRevisionConflict { expected, actual } => {
                Self::StaleStructureRevision { expected, actual }
            }
            CompletionCommitError::PlayerWorkRevisionConflict { expected, actual } => {
                Self::StalePlayerWorkRevision { expected, actual }
            }
            CompletionCommitError::SurvivalRevisionConflict { expected, actual } => {
                Self::StaleSurvivalRevision { expected, actual }
            }
            CompletionCommitError::Structure(error) => Self::Structure(error),
        }
    }
}

impl From<StorageEnclosureDismantlingTickError> for TickError {
    fn from(error: StorageEnclosureDismantlingTickError) -> Self {
        match error {
            StorageEnclosureDismantlingTickError::MaterialLotIds => Self::MaterialLotIdExhausted,
            StorageEnclosureDismantlingTickError::InventoryRevision => {
                Self::InventoryRevisionExhausted
            }
        }
    }
}

impl From<FieldProspectingTickError> for TickError {
    fn from(error: FieldProspectingTickError) -> Self {
        match error {
            FieldProspectingTickError::ObservationIdExhausted => {
                Self::GeologicalObservationIdExhausted
            }
            FieldProspectingTickError::KnowledgeRevisionExhausted => {
                Self::GeologicalKnowledgeRevisionExhausted
            }
        }
    }
}

impl From<ManualPowerTickError> for TickError {
    fn from(error: ManualPowerTickError) -> Self {
        match error {
            ManualPowerTickError::EnergyRevisionExhausted => {
                Self::ManualPowerEnergyRevisionExhausted
            }
            ManualPowerTickError::EquipmentRevisionExhausted => {
                Self::ManualPowerEquipmentRevisionExhausted
            }
        }
    }
}

impl From<MiningTickError> for TickError {
    fn from(error: MiningTickError) -> Self {
        match error {
            MiningTickError::Geology => Self::GeologyRevisionExhausted,
            MiningTickError::Mining => Self::MiningRevisionExhausted,
            MiningTickError::Equipment => Self::EquipmentRevisionExhausted,
        }
    }
}

impl From<SurvivalTickError> for TickError {
    fn from(error: SurvivalTickError) -> Self {
        match error {
            SurvivalTickError::RevisionExhausted => Self::SurvivalRevisionExhausted,
            SurvivalTickError::EnergyCostOverflow => Self::SurvivalEnergyCostOverflow,
            SurvivalTickError::HydrationCostOverflow => Self::SurvivalHydrationCostOverflow,
        }
    }
}
