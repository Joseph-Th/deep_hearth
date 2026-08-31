//! Diagnostics for same-material form-reform validation and commit.

use crate::core::quantity::Mass;
use crate::material::{CommodityKey, FormId, MaterialId};
use crate::structural::StructuralCommitError;

use super::super::super::state::StockpileId;
use super::super::super::storage_validation::StockpileStorageError;
use super::super::super::structural_integration::StockpileStructuralLoadError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MaterialReformError {
    StaleSelection {
        expected: u64,
        actual: u64,
    },
    UnknownSource {
        stockpile: StockpileId,
    },
    UnknownDestination {
        stockpile: StockpileId,
    },
    UnknownTargetMaterial {
        material: MaterialId,
    },
    UnknownTargetForm {
        form: FormId,
    },
    MaterialChanged {
        source: MaterialId,
        target: MaterialId,
    },
    PhaseChanged {
        source: FormId,
        target: FormId,
    },
    TargetUnchanged {
        commodity: CommodityKey,
    },
    DestinationStorage(StockpileStorageError),
    DestinationMassOverflow {
        stockpile: StockpileId,
    },
    DestinationCapacityExceeded {
        stockpile: StockpileId,
        capacity: Mass,
        committed: Mass,
        requested: Mass,
    },
    LotIdExhausted,
    RevisionExhausted,
    StructuralLoad(StockpileStructuralLoadError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MaterialReformCommitError {
    StaleInventoryRevision { expected: u64, actual: u64 },
    Structure(StructuralCommitError),
}
