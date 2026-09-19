//! Typed failures for trusted-load validation of persistent fluid state.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Temperature, Volume};
use crate::core::time::SimulationTick;
use crate::structural::StructuralElementId;

use super::super::super::definitions::FluidDefinitionId;
use super::super::FluidStoreId;

/// Invalid persisted fluid ownership discovered during exhaustive load validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FluidValidationError {
    InvalidIdCursor,
    ZeroStoreId,
    RecordKeyMismatch {
        key: FluidStoreId,
        record: FluidStoreId,
    },
    ZeroCapacity {
        store: FluidStoreId,
    },
    ZeroStoredVolume {
        store: FluidStoreId,
    },
    ZeroStoredTemperature {
        store: FluidStoreId,
    },
    CapacityExceeded {
        store: FluidStoreId,
        stored: Volume,
        capacity: Volume,
    },
    UnknownDefinition {
        store: FluidStoreId,
        definition: FluidDefinitionId,
    },
    StoredBelowMeltingPoint {
        store: FluidStoreId,
        definition: FluidDefinitionId,
        temperature: Temperature,
        melting_point: Temperature,
    },
    ZeroSupportElementId {
        store: FluidStoreId,
    },
    ZeroIndexedSupportElementId,
    ZeroIndexedStoreId {
        element: StructuralElementId,
    },
    EmptySupportIndex {
        element: StructuralElementId,
    },
    MissingSupportIndex {
        store: FluidStoreId,
        element: StructuralElementId,
    },
    UnknownIndexedStore {
        store: FluidStoreId,
        element: StructuralElementId,
    },
    SupportIndexMismatch {
        store: FluidStoreId,
        indexed: StructuralElementId,
        actual: Option<StructuralElementId>,
    },
    CreatedInFuture {
        store: FluidStoreId,
        created_at: SimulationTick,
        current: SimulationTick,
    },
}

impl Display for FluidValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidIdCursor => formatter.write_str("fluid store ID cursor is invalid"),
            Self::ZeroStoreId => formatter.write_str("fluid store ID must be nonzero"),
            Self::RecordKeyMismatch { key, record } => write!(
                formatter,
                "fluid store map key {} disagrees with record id {}",
                key.value(),
                record.value()
            ),
            Self::ZeroCapacity { store } => {
                write!(formatter, "fluid store {} has zero capacity", store.value())
            }
            Self::ZeroStoredVolume { store } => write!(
                formatter,
                "fluid store {} retains a fluid identity at zero volume",
                store.value()
            ),
            Self::ZeroStoredTemperature { store } => write!(
                formatter,
                "fluid store {} contains fluid at absolute zero",
                store.value()
            ),
            Self::CapacityExceeded {
                store,
                stored,
                capacity,
            } => write!(
                formatter,
                "fluid store {} contains {} uL above capacity {} uL",
                store.value(),
                stored.microliters(),
                capacity.microliters()
            ),
            Self::UnknownDefinition { store, definition } => write!(
                formatter,
                "fluid store {} references unknown fluid definition {}",
                store.value(),
                definition.value()
            ),
            Self::StoredBelowMeltingPoint {
                store,
                definition,
                temperature,
                melting_point,
            } => write!(
                formatter,
                "fluid store {} contains definition {} at {} mK below its material melting point {} mK",
                store.value(),
                definition.value(),
                temperature.millikelvin(),
                melting_point.millikelvin()
            ),
            Self::ZeroSupportElementId { store } => write!(
                formatter,
                "fluid store {} references zero structural support id",
                store.value()
            ),
            Self::ZeroIndexedSupportElementId => {
                formatter.write_str("fluid support reverse index contains zero structural id")
            }
            Self::ZeroIndexedStoreId { element } => write!(
                formatter,
                "fluid support reverse index for element {} contains zero store id",
                element.value()
            ),
            Self::EmptySupportIndex { element } => write!(
                formatter,
                "fluid support reverse index contains empty entry for element {}",
                element.value()
            ),
            Self::MissingSupportIndex { store, element } => write!(
                formatter,
                "fluid store {} references support element {} but is absent from the reverse index",
                store.value(),
                element.value()
            ),
            Self::UnknownIndexedStore { store, element } => write!(
                formatter,
                "fluid support reverse index element {} references missing store {}",
                element.value(),
                store.value()
            ),
            Self::SupportIndexMismatch {
                store,
                indexed,
                actual,
            } => write!(
                formatter,
                "fluid support reverse index places store {} on element {} but record support is {actual:?}",
                store.value(),
                indexed.value()
            ),
            Self::CreatedInFuture {
                store,
                created_at,
                current,
            } => write!(
                formatter,
                "fluid store {} was created at tick {} after current tick {}",
                store.value(),
                created_at.value(),
                current.value()
            ),
        }
    }
}

impl Error for FluidValidationError {}
