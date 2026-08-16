//! Persistent-state validation for fluid; this child audits private owner data without exposing mutation.

use super::*;

/// Invalid persisted fluid ownership discovered during exhaustive load validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FluidValidationError {
    InvalidIdCursor,
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
    CapacityExceeded {
        store: FluidStoreId,
        stored: Volume,
        capacity: Volume,
    },
    UnknownDefinition {
        store: FluidStoreId,
        definition: FluidDefinitionId,
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

pub(crate) fn validate_loaded_fluid(
    registry: &FluidRegistry,
    state: &FluidState,
    current: SimulationTick,
) -> Result<(), FluidValidationError> {
    if !state.has_valid_id_cursor() {
        return Err(FluidValidationError::InvalidIdCursor);
    }
    for (key, record) in &state.records {
        if *key != record.id {
            return Err(FluidValidationError::RecordKeyMismatch {
                key: *key,
                record: record.id,
            });
        }
        if record.capacity.is_zero() {
            return Err(FluidValidationError::ZeroCapacity { store: record.id });
        }
        if let Some(contents) = record.contents {
            if contents.volume.is_zero() {
                return Err(FluidValidationError::ZeroStoredVolume { store: record.id });
            }
            if contents.volume > record.capacity {
                return Err(FluidValidationError::CapacityExceeded {
                    store: record.id,
                    stored: contents.volume,
                    capacity: record.capacity,
                });
            }
            if registry.get_fluid(contents.fluid).is_none() {
                return Err(FluidValidationError::UnknownDefinition {
                    store: record.id,
                    definition: contents.fluid,
                });
            }
        }
        if record
            .supported_by
            .is_some_and(|element| element.value() == 0)
        {
            return Err(FluidValidationError::ZeroSupportElementId { store: record.id });
        }
        if let Some(element) = record.supported_by
            && !state
                .stores_by_support
                .get(&element)
                .is_some_and(|stores| stores.contains(&record.id))
        {
            return Err(FluidValidationError::MissingSupportIndex {
                store: record.id,
                element,
            });
        }
        if record.created_at > current {
            return Err(FluidValidationError::CreatedInFuture {
                store: record.id,
                created_at: record.created_at,
                current,
            });
        }
    }
    for (element, stores) in &state.stores_by_support {
        if element.value() == 0 {
            return Err(FluidValidationError::ZeroIndexedSupportElementId);
        }
        if stores.is_empty() {
            return Err(FluidValidationError::EmptySupportIndex { element: *element });
        }
        for store in stores {
            if store.value() == 0 {
                return Err(FluidValidationError::ZeroIndexedStoreId { element: *element });
            }
            let Some(record) = state.records.get(store) else {
                return Err(FluidValidationError::UnknownIndexedStore {
                    store: *store,
                    element: *element,
                });
            };
            if record.supported_by != Some(*element) {
                return Err(FluidValidationError::SupportIndexMismatch {
                    store: *store,
                    indexed: *element,
                    actual: record.supported_by,
                });
            }
        }
    }
    Ok(())
}
