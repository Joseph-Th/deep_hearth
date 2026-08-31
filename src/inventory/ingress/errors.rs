//! Diagnostics for canonical material ingress validation.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::time::SimulationTick;
use crate::material::{CompositionError, FormId, MaterialId};

use super::super::state::StockpileId;
use super::super::storage_validation::StockpileStorageError;

/// Failure while validating one complete source-owned material ingress transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MaterialIngressError {
    Empty,
    UnknownStockpile {
        stockpile: StockpileId,
    },
    UnknownMaterial {
        material: MaterialId,
    },
    UnknownForm {
        form: FormId,
    },
    UnknownCompositionMaterial {
        material: MaterialId,
    },
    ZeroMass,
    InvalidComposition {
        error: CompositionError,
    },
    CompositionMissingHost {
        host: MaterialId,
    },
    Storage(StockpileStorageError),
    InvalidProvenance,
    ProvenanceInFuture {
        latest: SimulationTick,
        current: SimulationTick,
    },
    MassOverflow {
        stockpile: StockpileId,
    },
    CapacityExceeded {
        stockpile: StockpileId,
        capacity: Mass,
        committed: Mass,
        requested: Mass,
    },
    LotIdExhausted,
    RevisionExhausted,
}

impl Display for MaterialIngressError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => formatter.write_str("material ingress must contain at least one parcel"),
            Self::UnknownStockpile { stockpile } => {
                write!(formatter, "unknown stockpile id {}", stockpile.value())
            }
            Self::UnknownMaterial { material } => {
                write!(formatter, "unknown material id {}", material.value())
            }
            Self::UnknownForm { form } => write!(formatter, "unknown form id {}", form.value()),
            Self::UnknownCompositionMaterial { material } => write!(
                formatter,
                "material ingress composition references unknown material {}",
                material.value()
            ),
            Self::ZeroMass => formatter.write_str("material ingress mass must be nonzero"),
            Self::InvalidComposition { error } => {
                write!(
                    formatter,
                    "material ingress has invalid composition: {error}"
                )
            }
            Self::CompositionMissingHost { host } => write!(
                formatter,
                "material ingress composition omits host material {}",
                host.value()
            ),
            Self::Storage(error) => {
                write!(formatter, "stockpile rejects material ingress: {error}")
            }
            Self::InvalidProvenance => formatter.write_str(
                "material ingress provenance ends before its earliest represented creation tick",
            ),
            Self::ProvenanceInFuture { latest, current } => write!(
                formatter,
                "material ingress provenance reaches tick {} after current tick {}",
                latest.value(),
                current.value()
            ),
            Self::MassOverflow { stockpile } => write!(
                formatter,
                "material ingress overflows mass accounting in stockpile {}",
                stockpile.value()
            ),
            Self::CapacityExceeded {
                stockpile,
                capacity,
                committed,
                requested,
            } => write!(
                formatter,
                "stockpile {} capacity {} mg exceeded: {} mg committed, {} mg ingress requested",
                stockpile.value(),
                capacity.milligrams(),
                committed.milligrams(),
                requested.milligrams()
            ),
            Self::LotIdExhausted => {
                formatter.write_str("material lot identifier space is exhausted")
            }
            Self::RevisionExhausted => formatter.write_str("inventory revision space is exhausted"),
        }
    }
}

impl Error for MaterialIngressError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidComposition { error } => Some(error),
            Self::Storage(error) => Some(error),
            Self::Empty
            | Self::UnknownStockpile { .. }
            | Self::UnknownMaterial { .. }
            | Self::UnknownForm { .. }
            | Self::UnknownCompositionMaterial { .. }
            | Self::ZeroMass
            | Self::CompositionMissingHost { .. }
            | Self::InvalidProvenance
            | Self::ProvenanceInFuture { .. }
            | Self::MassOverflow { .. }
            | Self::CapacityExceeded { .. }
            | Self::LotIdExhausted
            | Self::RevisionExhausted => None,
        }
    }
}
