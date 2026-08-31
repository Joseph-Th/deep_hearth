//! Diagnostics for production completion planning and revision-bound commit.

use crate::core::time::{SimulationTick, TickSpan};
use crate::inventory::{StockpileId, StockpileStructuralLoadError};
use crate::production::ProductionJobId;
use crate::structural::StructuralCommitError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CompletionPlanError {
    MaterialLotIds,
    InventoryRevision,
    ProductionRevision,
    EquipmentRevision,
    EnergyRevision,
    PlayerWorkRevision,
    ResumeTickOverflow {
        job: ProductionJobId,
        current: SimulationTick,
        remaining: TickSpan,
    },
    DestinationMassOverflow {
        stockpile: StockpileId,
    },
    StorageAgeOverflow {
        job: ProductionJobId,
    },
    StructuralLoad(StockpileStructuralLoadError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CompletionCommitError {
    InventoryStale { expected: u64, actual: u64 },
    ProductionRevisionChanged { expected: u64, actual: u64 },
    EquipmentRevisionConflict { expected: u64, actual: u64 },
    EnergyRevisionConflict { expected: u64, actual: u64 },
    StructureRevisionConflict { expected: u64, actual: u64 },
    PlayerWorkRevisionConflict { expected: u64, actual: u64 },
    SurvivalRevisionConflict { expected: u64, actual: u64 },
    Structure(StructuralCommitError),
}
