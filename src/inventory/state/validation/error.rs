//! Persistent-state failure vocabulary for inventory trusted-load validation.

use std::error::Error;

use crate::core::quantity::{Mass, Temperature};
use crate::core::time::SimulationTick;
use crate::material::{
    CommodityKey, CompositionError, FormId, MaterialId, MaterialPhase, MaterialPhaseStateError,
    ParticleSizeStateError,
};
use crate::structural::StructuralElementId;

use super::super::{MaterialLotId, StockpileId};

mod display;

/// Persistent-state validation failure for the inventory owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InventoryValidationError {
    ZeroNextStockpileId,
    ZeroNextLotId,
    ZeroStockpileId,
    ZeroLotId,
    NextIdNotAfterExisting {
        next: u32,
        highest: StockpileId,
    },
    NextLotIdNotAfterExisting {
        next: u64,
        highest: MaterialLotId,
    },
    IdMismatch {
        key: StockpileId,
        record: StockpileId,
    },
    ZeroCapacity {
        stockpile: StockpileId,
    },
    ZeroCommodityMass {
        stockpile: StockpileId,
        commodity: CommodityKey,
    },
    UnknownLotForm {
        lot: MaterialLotId,
        form: FormId,
    },
    LotPhaseNotAccepted {
        lot: MaterialLotId,
        stockpile: StockpileId,
        phase: MaterialPhase,
    },
    LotTemperatureExceedsStorageMaximum {
        lot: MaterialLotId,
        stockpile: StockpileId,
        temperature: Temperature,
        maximum: Temperature,
    },
    LotIdMismatch {
        key: MaterialLotId,
        record: MaterialLotId,
    },
    ZeroLotMass {
        lot: MaterialLotId,
    },
    InvalidLotComposition {
        lot: MaterialLotId,
        error: CompositionError,
    },
    LotCompositionMissingHost {
        lot: MaterialLotId,
        host: MaterialId,
    },
    UnsupportedLotCommodity {
        lot: MaterialLotId,
        commodity: CommodityKey,
    },
    InvalidLotPhaseState {
        lot: MaterialLotId,
        error: MaterialPhaseStateError,
    },
    InvalidLotParticleSizeState {
        lot: MaterialLotId,
        error: ParticleSizeStateError,
    },
    LotProvenanceInFuture {
        lot: MaterialLotId,
        latest: SimulationTick,
        current: SimulationTick,
    },
    LotStorageTransitionBeforeCreation {
        lot: MaterialLotId,
        transition: SimulationTick,
        created: SimulationTick,
    },
    LotStorageTransitionInFuture {
        lot: MaterialLotId,
        transition: SimulationTick,
        current: SimulationTick,
    },
    MissingLotOwner {
        lot: MaterialLotId,
        stockpile: StockpileId,
    },
    LotIndexMismatch {
        stockpile: StockpileId,
    },
    CommodityMassMismatch {
        stockpile: StockpileId,
        commodity: CommodityKey,
        cached: Mass,
        calculated: Mass,
    },
    StoredMassMismatch {
        stockpile: StockpileId,
        cached: Mass,
        calculated: Mass,
    },
    CapacityExceeded {
        stockpile: StockpileId,
    },
    MassOverflow {
        stockpile: StockpileId,
    },
    ZeroSupportElementId {
        stockpile: StockpileId,
    },
    ZeroIndexedSupportElementId,
    EmptySupportIndex {
        element: StructuralElementId,
    },
    MissingSupportIndex {
        stockpile: StockpileId,
        element: StructuralElementId,
    },
    UnknownIndexedStockpile {
        stockpile: StockpileId,
        element: StructuralElementId,
    },
    SupportIndexMismatch {
        stockpile: StockpileId,
        indexed: StructuralElementId,
        actual: Option<StructuralElementId>,
    },
}

impl Error for InventoryValidationError {}
