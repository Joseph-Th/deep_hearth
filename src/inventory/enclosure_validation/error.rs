//! Typed failures for trusted-load validation of material-backed storage enclosures.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::time::SimulationTick;
use crate::material::{CommodityKey, MaterialPhaseStateError, ParticleSizeStateError};

use super::super::{StockpileId, StockpileStorageProfile, StorageDefinitionId};

/// Invalid persisted state for one stockpile's material-backed storage enclosure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StorageEnclosureValidationError {
    UnknownDefinition {
        stockpile: StockpileId,
        definition: StorageDefinitionId,
    },
    StorageProfileMismatch {
        stockpile: StockpileId,
        stored: StockpileStorageProfile,
        authored: StockpileStorageProfile,
    },
    CapacityExceeded {
        stockpile: StockpileId,
        capacity: Mass,
        maximum: Mass,
    },
    ConstructionInFuture {
        stockpile: StockpileId,
        created_at: SimulationTick,
        current: SimulationTick,
    },
    MissingEmbodiedMaterial {
        stockpile: StockpileId,
    },
    ZeroEmbodiedTrace {
        stockpile: StockpileId,
    },
    UnknownEmbodiedCommodity {
        stockpile: StockpileId,
        commodity: CommodityKey,
    },
    ImpureEmbodiedMaterial {
        stockpile: StockpileId,
        commodity: CommodityKey,
    },
    InvalidEmbodiedPhaseState {
        stockpile: StockpileId,
        error: MaterialPhaseStateError,
    },
    InvalidEmbodiedParticleSizeState {
        stockpile: StockpileId,
        error: ParticleSizeStateError,
    },
    EmbodiedProvenanceInFuture {
        stockpile: StockpileId,
        latest_created_at: SimulationTick,
        current: SimulationTick,
    },
    EmbodiedProvenanceAfterConstruction {
        stockpile: StockpileId,
        latest_created_at: SimulationTick,
        created_at: SimulationTick,
    },
    EmbodiedTraceMassOverflow {
        stockpile: StockpileId,
    },
    EmbodiedMassMismatch {
        stockpile: StockpileId,
        traced: Mass,
        authored: Mass,
    },
    AssemblyMaterialMismatch {
        stockpile: StockpileId,
        commodity: CommodityKey,
        stored: Mass,
        authored: Mass,
    },
}

impl Display for StorageEnclosureValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownDefinition {
                stockpile,
                definition,
            } => write!(
                formatter,
                "stockpile {} references unknown storage enclosure definition {}",
                stockpile.value(),
                definition.value()
            ),
            Self::StorageProfileMismatch { stockpile, .. } => write!(
                formatter,
                "stockpile {} storage profile disagrees with its enclosure definition",
                stockpile.value()
            ),
            Self::CapacityExceeded {
                stockpile,
                capacity,
                maximum,
            } => write!(
                formatter,
                "stockpile {} capacity {} mg exceeds enclosure maximum {} mg",
                stockpile.value(),
                capacity.milligrams(),
                maximum.milligrams()
            ),
            Self::ConstructionInFuture {
                stockpile,
                created_at,
                current,
            } => write!(
                formatter,
                "stockpile {} enclosure was created at tick {} after current tick {}",
                stockpile.value(),
                created_at.value(),
                current.value()
            ),
            Self::MissingEmbodiedMaterial { stockpile } => write!(
                formatter,
                "stockpile {} enclosure has no embodied construction traces",
                stockpile.value()
            ),
            Self::ZeroEmbodiedTrace { stockpile } => write!(
                formatter,
                "stockpile {} enclosure contains a zero-mass construction trace",
                stockpile.value()
            ),
            Self::UnknownEmbodiedCommodity {
                stockpile,
                commodity,
            } => write!(
                formatter,
                "stockpile {} enclosure contains unknown construction commodity {}",
                stockpile.value(),
                commodity.value()
            ),
            Self::ImpureEmbodiedMaterial {
                stockpile,
                commodity,
            } => write!(
                formatter,
                "stockpile {} enclosure construction commodity {} is not pure host material",
                stockpile.value(),
                commodity.value()
            ),
            Self::InvalidEmbodiedPhaseState { stockpile, error } => write!(
                formatter,
                "stockpile {} enclosure has invalid construction phase state: {error}",
                stockpile.value()
            ),
            Self::InvalidEmbodiedParticleSizeState { stockpile, error } => write!(
                formatter,
                "stockpile {} enclosure has invalid construction particle state: {error}",
                stockpile.value()
            ),
            Self::EmbodiedProvenanceInFuture {
                stockpile,
                latest_created_at,
                current,
            } => write!(
                formatter,
                "stockpile {} enclosure construction matter was created at tick {} after current tick {}",
                stockpile.value(),
                latest_created_at.value(),
                current.value()
            ),
            Self::EmbodiedProvenanceAfterConstruction {
                stockpile,
                latest_created_at,
                created_at,
            } => write!(
                formatter,
                "stockpile {} enclosure contains matter created at tick {} after enclosure construction tick {}",
                stockpile.value(),
                latest_created_at.value(),
                created_at.value()
            ),
            Self::EmbodiedTraceMassOverflow { stockpile } => write!(
                formatter,
                "stockpile {} enclosure construction trace mass overflowed",
                stockpile.value()
            ),
            Self::EmbodiedMassMismatch {
                stockpile,
                traced,
                authored,
            } => write!(
                formatter,
                "stockpile {} enclosure traces {} mg embodied matter but definition requires {} mg",
                stockpile.value(),
                traced.milligrams(),
                authored.milligrams()
            ),
            Self::AssemblyMaterialMismatch {
                stockpile,
                commodity,
                stored,
                authored,
            } => write!(
                formatter,
                "stockpile {} enclosure traces {} mg of commodity {} but definition requires {} mg",
                stockpile.value(),
                stored.milligrams(),
                commodity.value(),
                authored.milligrams()
            ),
        }
    }
}

impl Error for StorageEnclosureValidationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidEmbodiedPhaseState { error, .. } => Some(error),
            Self::InvalidEmbodiedParticleSizeState { error, .. } => Some(error),
            Self::UnknownDefinition { .. }
            | Self::StorageProfileMismatch { .. }
            | Self::CapacityExceeded { .. }
            | Self::ConstructionInFuture { .. }
            | Self::MissingEmbodiedMaterial { .. }
            | Self::ZeroEmbodiedTrace { .. }
            | Self::UnknownEmbodiedCommodity { .. }
            | Self::ImpureEmbodiedMaterial { .. }
            | Self::EmbodiedProvenanceInFuture { .. }
            | Self::EmbodiedProvenanceAfterConstruction { .. }
            | Self::EmbodiedTraceMassOverflow { .. }
            | Self::EmbodiedMassMismatch { .. }
            | Self::AssemblyMaterialMismatch { .. } => None,
        }
    }
}
