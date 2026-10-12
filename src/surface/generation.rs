//! Validated admission of finite world-generated loose matter into surface ownership.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Mass, Temperature};
use crate::core::state::AppState;
use crate::material::{CommodityKey, CompositionError, MaterialComposition, MaterialId};
use crate::registry::Registries;
use crate::spatial::VoxelCoord;

use super::state::validation::{SurfaceResourceValidationError, validate_surface_resource_record};
use super::state::{SurfaceResourceId, SurfaceResourceRecord};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GeneratedSurfaceResourceSpec {
    position: VoxelCoord,
    commodity: CommodityKey,
    mass: Mass,
    temperature: Temperature,
    composition: MaterialComposition,
}

impl GeneratedSurfaceResourceSpec {
    pub(crate) fn new(
        position: VoxelCoord,
        commodity: CommodityKey,
        mass: Mass,
        temperature: Temperature,
        composition: MaterialComposition,
    ) -> Result<Self, GeneratedSurfaceResourceSpecError> {
        if mass.is_zero() {
            return Err(GeneratedSurfaceResourceSpecError::ZeroMass);
        }
        composition
            .validate()
            .map_err(GeneratedSurfaceResourceSpecError::InvalidComposition)?;
        if composition.parts_per_million(commodity.material()) == 0 {
            return Err(GeneratedSurfaceResourceSpecError::MissingHostMaterial {
                host: commodity.material(),
            });
        }
        Ok(Self {
            position,
            commodity,
            mass,
            temperature,
            composition,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GeneratedSurfaceResourceSpecError {
    ZeroMass,
    InvalidComposition(CompositionError),
    MissingHostMaterial { host: MaterialId },
}

impl Display for GeneratedSurfaceResourceSpecError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroMass => {
                formatter.write_str("generated surface resource mass must be nonzero")
            }
            Self::InvalidComposition(error) => {
                write!(formatter, "invalid generated surface composition: {error}")
            }
            Self::MissingHostMaterial { host } => write!(
                formatter,
                "generated surface composition omits host material {}",
                host.value()
            ),
        }
    }
}

impl Error for GeneratedSurfaceResourceSpecError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidComposition(error) => Some(error),
            Self::ZeroMass | Self::MissingHostMaterial { .. } => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SurfaceGenerationError {
    Empty,
    IdExhausted,
    RevisionExhausted,
    InvalidResource(SurfaceResourceValidationError),
}

impl Display for SurfaceGenerationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => formatter.write_str("surface generation batch must contain a resource"),
            Self::IdExhausted => {
                formatter.write_str("surface resource identifier space is exhausted")
            }
            Self::RevisionExhausted => {
                formatter.write_str("surface resource revision space is exhausted")
            }
            Self::InvalidResource(error) => {
                write!(formatter, "generated surface resource is invalid: {error}")
            }
        }
    }
}

impl Error for SurfaceGenerationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidResource(error) => Some(error),
            Self::Empty | Self::IdExhausted | Self::RevisionExhausted => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SurfaceGenerationCommitError {
    Tick {
        expected: crate::core::time::SimulationTick,
        actual: crate::core::time::SimulationTick,
    },
    Revision {
        expected: u64,
        actual: u64,
    },
    NextResourceId {
        expected: u32,
        actual: u32,
    },
}

impl Display for SurfaceGenerationCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "surface generation became stale: {self:?}")
    }
}

impl Error for SurfaceGenerationCommitError {}

#[must_use]
pub(crate) struct ValidatedSurfaceGeneration {
    expected_tick: crate::core::time::SimulationTick,
    expected_revision: u64,
    expected_next_resource_id: u32,
    records: Vec<SurfaceResourceRecord>,
    next_resource_id: u32,
    next_revision: u64,
}

impl ValidatedSurfaceGeneration {
    pub(crate) fn precheck(&self, state: &AppState) -> Result<(), SurfaceGenerationCommitError> {
        let actual_tick = state.tick();
        if actual_tick != self.expected_tick {
            return Err(SurfaceGenerationCommitError::Tick {
                expected: self.expected_tick,
                actual: actual_tick,
            });
        }
        let actual_revision = state.surface().revision();
        if actual_revision != self.expected_revision {
            return Err(SurfaceGenerationCommitError::Revision {
                expected: self.expected_revision,
                actual: actual_revision,
            });
        }
        let actual_next_resource_id = state.surface().next_resource_id();
        if actual_next_resource_id != self.expected_next_resource_id {
            return Err(SurfaceGenerationCommitError::NextResourceId {
                expected: self.expected_next_resource_id,
                actual: actual_next_resource_id,
            });
        }
        Ok(())
    }

    pub(crate) fn apply(self, state: &mut AppState) {
        state.surface_state_mut().apply_generated_batch(
            self.records,
            self.next_resource_id,
            self.next_revision,
        );
    }

    #[cfg(any(test, feature = "test-gameplay"))]
    pub(crate) fn commit(self, state: &mut AppState) -> Result<(), SurfaceGenerationCommitError> {
        self.precheck(state)?;
        self.apply(state);
        Ok(())
    }
}

pub(crate) fn validate_generate_surface_resources(
    registries: &Registries,
    state: &AppState,
    specs: impl IntoIterator<Item = GeneratedSurfaceResourceSpec>,
) -> Result<ValidatedSurfaceGeneration, SurfaceGenerationError> {
    let specs = specs.into_iter().collect::<Vec<_>>();
    if specs.is_empty() {
        return Err(SurfaceGenerationError::Empty);
    }
    let expected_revision = state.surface().revision();
    let expected_next_resource_id = state.surface().next_resource_id();
    let next_revision = expected_revision
        .checked_add(1)
        .ok_or(SurfaceGenerationError::RevisionExhausted)?;
    let generated_at = state.tick();
    let mut next_resource_id = expected_next_resource_id;
    let mut records = Vec::with_capacity(specs.len());
    for spec in specs {
        let id = SurfaceResourceId::new(next_resource_id);
        next_resource_id = next_resource_id
            .checked_add(1)
            .ok_or(SurfaceGenerationError::IdExhausted)?;
        let record = SurfaceResourceRecord::generated(
            id,
            spec.position,
            spec.commodity,
            spec.mass,
            spec.temperature,
            spec.composition,
            generated_at,
        );
        validate_surface_resource_record(registries.materials(), id, &record, generated_at)
            .map_err(SurfaceGenerationError::InvalidResource)?;
        records.push(record);
    }
    Ok(ValidatedSurfaceGeneration {
        expected_tick: generated_at,
        expected_revision,
        expected_next_resource_id,
        records,
        next_resource_id,
        next_revision,
    })
}

#[cfg(test)]
pub(crate) fn insert_generated_surface_resource(
    registries: &Registries,
    state: &mut AppState,
    spec: GeneratedSurfaceResourceSpec,
) -> Result<SurfaceResourceId, SurfaceGenerationError> {
    let id = SurfaceResourceId::new(state.surface().next_resource_id());
    validate_generate_surface_resources(registries, state, [spec])?
        .commit(state)
        .unwrap_or_else(|error| {
            unreachable!("immediate test surface generation cannot become stale: {error}")
        });
    Ok(id)
}
