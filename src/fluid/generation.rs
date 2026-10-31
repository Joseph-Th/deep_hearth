//! Validates finite fluid stores created by authoritative world-generation boundaries.

use crate::core::quantity::{Temperature, Volume};
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::registry::Registries;

use super::{FluidContents, FluidDefinitionId, FluidStoreId, FluidStoreRecord};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GeneratedFluidStoreSpec {
    capacity: Volume,
    fluid: FluidDefinitionId,
    volume: Volume,
    temperature: Temperature,
}

impl GeneratedFluidStoreSpec {
    #[must_use]
    pub(crate) const fn new(
        capacity: Volume,
        fluid: FluidDefinitionId,
        volume: Volume,
        temperature: Temperature,
    ) -> Self {
        Self {
            capacity,
            fluid,
            volume,
            temperature,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FluidGenerationError {
    ZeroCapacity,
    UnknownDefinition {
        definition: FluidDefinitionId,
    },
    ZeroVolume,
    ZeroTemperature,
    VolumeExceedsCapacity {
        volume: Volume,
        capacity: Volume,
    },
    BelowMeltingPoint {
        definition: FluidDefinitionId,
        temperature: Temperature,
        melting_point: Temperature,
    },
    IdExhausted,
    RevisionExhausted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FluidGenerationCommitError {
    Tick {
        expected: SimulationTick,
        actual: SimulationTick,
    },
    Revision {
        expected: u64,
        actual: u64,
    },
    NextStoreId {
        expected: u64,
        actual: u64,
    },
}

#[must_use]
pub(crate) struct ValidatedFluidGeneration {
    expected_tick: SimulationTick,
    expected_revision: u64,
    expected_next_store_id: u64,
    record: FluidStoreRecord,
    next_store_id: u64,
    next_revision: u64,
}

/// Batch of finite water pockets admitted against one world snapshot.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ValidatedFluidGenerationBatch {
    expected_tick: SimulationTick,
    expected_revision: u64,
    expected_next_store_id: u64,
    records: Vec<FluidStoreRecord>,
    next_store_id: u64,
    next_revision: u64,
}

impl ValidatedFluidGenerationBatch {
    pub(crate) fn precheck(&self, state: &AppState) -> Result<(), FluidGenerationCommitError> {
        if state.tick() != self.expected_tick {
            return Err(FluidGenerationCommitError::Tick {
                expected: self.expected_tick,
                actual: state.tick(),
            });
        }
        if state.fluid().revision() != self.expected_revision {
            return Err(FluidGenerationCommitError::Revision {
                expected: self.expected_revision,
                actual: state.fluid().revision(),
            });
        }
        if state.fluid().next_store_id() != self.expected_next_store_id {
            return Err(FluidGenerationCommitError::NextStoreId {
                expected: self.expected_next_store_id,
                actual: state.fluid().next_store_id(),
            });
        }
        Ok(())
    }

    pub(crate) fn store_ids(&self) -> impl ExactSizeIterator<Item = FluidStoreId> + '_ {
        self.records.iter().map(FluidStoreRecord::id)
    }

    pub(crate) fn apply(self, state: &mut AppState) {
        state.fluid_state_mut().insert_generated_batch(
            self.records,
            self.next_store_id,
            self.next_revision,
        );
    }
}

pub(crate) fn validate_generate_fluid_stores(
    registries: &Registries,
    state: &AppState,
    specs: impl IntoIterator<Item = GeneratedFluidStoreSpec>,
) -> Result<ValidatedFluidGenerationBatch, FluidGenerationError> {
    let mut records = Vec::new();
    let fluid = state.fluid();
    let mut next_store_id = fluid.next_store_id();
    for spec in specs {
        let contents = validate_generated_contents(registries, spec)?;
        let id = FluidStoreId::new(next_store_id);
        next_store_id = next_store_id
            .checked_add(1)
            .ok_or(FluidGenerationError::IdExhausted)?;
        records.push(FluidStoreRecord {
            id,
            capacity: spec.capacity,
            contents: Some(contents),
            supported_by: None,
            created_at: state.tick(),
        });
    }
    assert!(!records.is_empty(), "generated fluid batch requires water");
    let next_revision = fluid
        .revision()
        .checked_add(1)
        .ok_or(FluidGenerationError::RevisionExhausted)?;
    Ok(ValidatedFluidGenerationBatch {
        expected_tick: state.tick(),
        expected_revision: fluid.revision(),
        expected_next_store_id: fluid.next_store_id(),
        records,
        next_store_id,
        next_revision,
    })
}

impl ValidatedFluidGeneration {
    #[must_use]
    pub(crate) const fn store(&self) -> FluidStoreId {
        self.record.id
    }

    pub(crate) fn precheck(&self, state: &AppState) -> Result<(), FluidGenerationCommitError> {
        let actual_tick = state.tick();
        if actual_tick != self.expected_tick {
            return Err(FluidGenerationCommitError::Tick {
                expected: self.expected_tick,
                actual: actual_tick,
            });
        }
        let actual_revision = state.fluid().revision();
        if actual_revision != self.expected_revision {
            return Err(FluidGenerationCommitError::Revision {
                expected: self.expected_revision,
                actual: actual_revision,
            });
        }
        let actual_next_store_id = state.fluid().next_store_id();
        if actual_next_store_id != self.expected_next_store_id {
            return Err(FluidGenerationCommitError::NextStoreId {
                expected: self.expected_next_store_id,
                actual: actual_next_store_id,
            });
        }
        Ok(())
    }

    pub(crate) fn apply(self, state: &mut AppState) {
        state
            .fluid_state_mut()
            .insert_store(self.record, self.next_store_id, self.next_revision);
    }

    #[cfg(any(test, feature = "test-gameplay"))]
    pub(crate) fn commit(
        self,
        state: &mut AppState,
    ) -> Result<FluidStoreId, FluidGenerationCommitError> {
        self.precheck(state)?;
        let store = self.store();
        self.apply(state);
        Ok(store)
    }
}

fn validate_generated_contents(
    registries: &Registries,
    spec: GeneratedFluidStoreSpec,
) -> Result<FluidContents, FluidGenerationError> {
    if spec.capacity.is_zero() {
        return Err(FluidGenerationError::ZeroCapacity);
    }
    if spec.volume.is_zero() {
        return Err(FluidGenerationError::ZeroVolume);
    }
    if spec.temperature == Temperature::ZERO {
        return Err(FluidGenerationError::ZeroTemperature);
    }
    if spec.volume > spec.capacity {
        return Err(FluidGenerationError::VolumeExceedsCapacity {
            volume: spec.volume,
            capacity: spec.capacity,
        });
    }
    let definition = registries.fluid().get_fluid(spec.fluid).ok_or(
        FluidGenerationError::UnknownDefinition {
            definition: spec.fluid,
        },
    )?;
    if let Some(melting_point) = definition.minimum_modeled_temperature(registries.materials())
        && spec.temperature < melting_point
    {
        return Err(FluidGenerationError::BelowMeltingPoint {
            definition: spec.fluid,
            temperature: spec.temperature,
            melting_point,
        });
    }
    Ok(FluidContents {
        fluid: spec.fluid,
        volume: spec.volume,
        temperature: spec.temperature,
    })
}

pub(crate) fn validate_generate_fluid_store(
    registries: &Registries,
    state: &AppState,
    spec: GeneratedFluidStoreSpec,
) -> Result<ValidatedFluidGeneration, FluidGenerationError> {
    let contents = validate_generated_contents(registries, spec)?;

    let fluid = state.fluid();
    let id = FluidStoreId::new(fluid.next_store_id());
    let next_store_id = fluid
        .next_store_id()
        .checked_add(1)
        .ok_or(FluidGenerationError::IdExhausted)?;
    let next_revision = fluid
        .revision()
        .checked_add(1)
        .ok_or(FluidGenerationError::RevisionExhausted)?;
    Ok(ValidatedFluidGeneration {
        expected_tick: state.tick(),
        expected_revision: fluid.revision(),
        expected_next_store_id: fluid.next_store_id(),
        record: FluidStoreRecord {
            id,
            capacity: spec.capacity,
            contents: Some(contents),
            supported_by: None,
            created_at: state.tick(),
        },
        next_store_id,
        next_revision,
    })
}
