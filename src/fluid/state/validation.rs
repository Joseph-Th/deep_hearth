//! Validates persisted fluid stores, contents, support indexes, authored references, and cursors.

use crate::core::quantity::Temperature;
use crate::core::time::SimulationTick;
use crate::material::MaterialRegistry;
use crate::structural::{SupportIndexValidationFault, validate_support_index};

use super::super::definitions::FluidRegistry;
use super::{FluidState, FluidStoreId, FluidStoreRecord};

mod error;

pub use error::FluidValidationError;

pub(crate) fn validate_loaded_fluid(
    registry: &FluidRegistry,
    materials: &MaterialRegistry,
    state: &FluidState,
    current: SimulationTick,
) -> Result<(), FluidValidationError> {
    if !state.has_valid_id_cursor() {
        return Err(FluidValidationError::InvalidIdCursor);
    }
    for (key, record) in &state.records {
        validate_fluid_store(registry, materials, state, *key, record, current)?;
    }
    validate_fluid_support_index(state)
}

fn validate_fluid_store(
    registry: &FluidRegistry,
    materials: &MaterialRegistry,
    state: &FluidState,
    key: FluidStoreId,
    record: &FluidStoreRecord,
    current: SimulationTick,
) -> Result<(), FluidValidationError> {
    validate_fluid_store_identity(key, record.id)?;
    if record.capacity.is_zero() {
        return Err(FluidValidationError::ZeroCapacity { store: record.id });
    }
    validate_fluid_contents(registry, materials, record)?;
    validate_fluid_support_reference(state, record)?;
    if record.created_at > current {
        return Err(FluidValidationError::CreatedInFuture {
            store: record.id,
            created_at: record.created_at,
            current,
        });
    }
    Ok(())
}

fn validate_fluid_store_identity(
    key: FluidStoreId,
    record: FluidStoreId,
) -> Result<(), FluidValidationError> {
    if key.value() == 0 || record.value() == 0 {
        return Err(FluidValidationError::ZeroStoreId);
    }
    if key != record {
        return Err(FluidValidationError::RecordKeyMismatch { key, record });
    }
    Ok(())
}

fn validate_fluid_contents(
    registry: &FluidRegistry,
    materials: &MaterialRegistry,
    record: &FluidStoreRecord,
) -> Result<(), FluidValidationError> {
    let Some(contents) = record.contents else {
        return Ok(());
    };
    if contents.volume.is_zero() {
        return Err(FluidValidationError::ZeroStoredVolume { store: record.id });
    }
    if contents.temperature == Temperature::ZERO {
        return Err(FluidValidationError::ZeroStoredTemperature { store: record.id });
    }
    if contents.volume > record.capacity {
        return Err(FluidValidationError::CapacityExceeded {
            store: record.id,
            stored: contents.volume,
            capacity: record.capacity,
        });
    }
    let definition =
        registry
            .get_fluid(contents.fluid)
            .ok_or(FluidValidationError::UnknownDefinition {
                store: record.id,
                definition: contents.fluid,
            })?;
    if let Some(melting_point) = definition.minimum_modeled_temperature(materials)
        && contents.temperature < melting_point
    {
        return Err(FluidValidationError::StoredBelowMeltingPoint {
            store: record.id,
            definition: contents.fluid,
            temperature: contents.temperature,
            melting_point,
        });
    }
    Ok(())
}

fn validate_fluid_support_reference(
    state: &FluidState,
    record: &FluidStoreRecord,
) -> Result<(), FluidValidationError> {
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
    Ok(())
}

fn validate_fluid_support_index(state: &FluidState) -> Result<(), FluidValidationError> {
    validate_support_index(
        &state.stores_by_support,
        |store| store.value() == 0,
        |store| state.records.get(&store).map(|record| record.supported_by),
    )
    .map_err(|fault| match fault {
        SupportIndexValidationFault::ZeroSupportElementId => {
            FluidValidationError::ZeroIndexedSupportElementId
        }
        SupportIndexValidationFault::EmptySupportBucket { element } => {
            FluidValidationError::EmptySupportIndex { element }
        }
        SupportIndexValidationFault::InvalidItemId { element, .. } => {
            FluidValidationError::ZeroIndexedStoreId { element }
        }
        SupportIndexValidationFault::UnknownIndexedItem { item, element } => {
            FluidValidationError::UnknownIndexedStore {
                store: item,
                element,
            }
        }
        SupportIndexValidationFault::SupportMismatch {
            item,
            indexed,
            actual,
        } => FluidValidationError::SupportIndexMismatch {
            store: item,
            indexed,
            actual,
        },
    })
}

#[cfg(test)]
#[path = "validation_tests.rs"]
mod tests;
