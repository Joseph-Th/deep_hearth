//! Controlled finite fluid-store allocation for tests and gameplay audit fixtures.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Temperature, Volume};
use crate::core::state::AppState;
use crate::registry::Registries;

use super::definitions::FluidDefinitionId;
use super::state::{FluidContents, FluidStoreId, FluidStoreRecord};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AddFluidStoreError {
    ZeroCapacity,
    UnknownDefinition {
        definition: FluidDefinitionId,
    },
    InitialVolumeZero,
    InitialTemperatureZero,
    InitialBelowMeltingPoint {
        definition: FluidDefinitionId,
        temperature: Temperature,
        melting_point: Temperature,
    },
    InitialVolumeExceedsCapacity {
        initial: Volume,
        capacity: Volume,
    },
    IdExhausted,
    RevisionExhausted,
}

impl Display for AddFluidStoreError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroCapacity => formatter.write_str("fluid store capacity must be nonzero"),
            Self::UnknownDefinition { definition } => {
                write!(formatter, "unknown fluid definition {}", definition.value())
            }
            Self::InitialVolumeZero => formatter.write_str("initial fluid volume must be nonzero"),
            Self::InitialTemperatureZero => {
                formatter.write_str("initial fluid temperature must be above absolute zero")
            }
            Self::InitialBelowMeltingPoint {
                definition,
                temperature,
                melting_point,
            } => write!(
                formatter,
                "initial fluid definition {} temperature {} mK is below its material melting point {} mK",
                definition.value(),
                temperature.millikelvin(),
                melting_point.millikelvin()
            ),
            Self::InitialVolumeExceedsCapacity { initial, capacity } => write!(
                formatter,
                "initial fluid volume {} uL exceeds store capacity {} uL",
                initial.microliters(),
                capacity.microliters()
            ),
            Self::IdExhausted => formatter.write_str("fluid store identifier space is exhausted"),
            Self::RevisionExhausted => {
                formatter.write_str("fluid state revision space is exhausted")
            }
        }
    }
}

impl Error for AddFluidStoreError {}

#[cfg(test)]
pub(crate) fn add_fluid_store(
    state: &mut AppState,
    capacity: Volume,
) -> Result<FluidStoreId, AddFluidStoreError> {
    allocate_fluid_store(state, capacity, None)
}

fn allocate_fluid_store(
    state: &mut AppState,
    capacity: Volume,
    contents: Option<FluidContents>,
) -> Result<FluidStoreId, AddFluidStoreError> {
    if capacity.is_zero() {
        return Err(AddFluidStoreError::ZeroCapacity);
    }
    if let Some(contents) = contents {
        if contents.volume.is_zero() {
            return Err(AddFluidStoreError::InitialVolumeZero);
        }
        if contents.temperature == Temperature::ZERO {
            return Err(AddFluidStoreError::InitialTemperatureZero);
        }
        if contents.volume > capacity {
            return Err(AddFluidStoreError::InitialVolumeExceedsCapacity {
                initial: contents.volume,
                capacity,
            });
        }
    }
    let fluid = state.fluid();
    let id = FluidStoreId::new(fluid.next_store_id());
    let next_store_id = fluid
        .next_store_id()
        .checked_add(1)
        .ok_or(AddFluidStoreError::IdExhausted)?;
    let next_revision = fluid
        .revision()
        .checked_add(1)
        .ok_or(AddFluidStoreError::RevisionExhausted)?;
    let record = FluidStoreRecord {
        id,
        capacity,
        contents,
        supported_by: None,
        created_at: state.tick(),
    };

    state
        .fluid_state_mut()
        .insert_store(record, next_store_id, next_revision);
    Ok(id)
}

pub(crate) fn add_fluid_store_with_contents_for_fixture(
    registries: &Registries,
    state: &mut AppState,
    capacity: Volume,
    definition: FluidDefinitionId,
    volume: Volume,
    temperature: Temperature,
) -> Result<FluidStoreId, AddFluidStoreError> {
    let definition_record = registries
        .fluid()
        .get_fluid(definition)
        .ok_or(AddFluidStoreError::UnknownDefinition { definition })?;
    if temperature == Temperature::ZERO {
        return Err(AddFluidStoreError::InitialTemperatureZero);
    }
    if let Some(melting_point) =
        definition_record.minimum_modeled_temperature(registries.materials())
        && temperature < melting_point
    {
        return Err(AddFluidStoreError::InitialBelowMeltingPoint {
            definition,
            temperature,
            melting_point,
        });
    }
    allocate_fluid_store(
        state,
        capacity,
        Some(FluidContents {
            fluid: definition,
            volume,
            temperature,
        }),
    )
}
