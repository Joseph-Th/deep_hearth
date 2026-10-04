//! Persistent finite loose surface-resource ownership.

use std::collections::BTreeMap;
#[cfg(any(test, feature = "test-gameplay"))]
use std::error::Error;
use std::fmt::Debug;
#[cfg(any(test, feature = "test-gameplay"))]
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize, Serializer};

use crate::core::quantity::{Mass, Temperature};
#[cfg(any(test, feature = "test-gameplay"))]
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::material::{CommodityKey, MaterialComposition};
#[cfg(any(test, feature = "test-gameplay"))]
use crate::material::{CompositionError, MaterialId};
#[cfg(any(test, feature = "test-gameplay"))]
use crate::registry::Registries;
use crate::spatial::VoxelCoord;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SurfaceResourceId(u32);

impl SurfaceResourceId {
    #[must_use]
    pub const fn new(value: u32) -> Self {
        assert!(value != 0, "surface resource id must be nonzero");
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SurfaceResourceLifecycle {
    Available,
    Depleted,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SurfaceResourceRecord {
    id: SurfaceResourceId,
    position: VoxelCoord,
    commodity: CommodityKey,
    initial_mass: Mass,
    remaining_mass: Mass,
    temperature: Temperature,
    composition: MaterialComposition,
    lifecycle: SurfaceResourceLifecycle,
    generated_at: SimulationTick,
    depleted_at: Option<SimulationTick>,
}

impl SurfaceResourceRecord {
    #[must_use]
    pub const fn id(&self) -> SurfaceResourceId {
        self.id
    }
    #[must_use]
    pub const fn position(&self) -> VoxelCoord {
        self.position
    }
    #[must_use]
    pub const fn commodity(&self) -> CommodityKey {
        self.commodity
    }
    #[must_use]
    pub const fn initial_mass(&self) -> Mass {
        self.initial_mass
    }
    #[must_use]
    pub const fn remaining_mass(&self) -> Mass {
        self.remaining_mass
    }
    #[must_use]
    pub const fn temperature(&self) -> Temperature {
        self.temperature
    }
    #[must_use]
    pub const fn composition(&self) -> &MaterialComposition {
        &self.composition
    }
    #[must_use]
    pub const fn lifecycle(&self) -> SurfaceResourceLifecycle {
        self.lifecycle
    }
    #[must_use]
    pub(crate) const fn generated_at(&self) -> SimulationTick {
        self.generated_at
    }
}

#[derive(Deserialize)]
#[cfg_attr(any(test, feature = "test-gameplay"), derive(Clone, PartialEq, Eq))]
#[serde(deny_unknown_fields)]
pub struct SurfaceResourceState {
    revision: u64,
    next_resource_id: u32,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_btree_map_no_duplicates")]
    resources: BTreeMap<SurfaceResourceId, SurfaceResourceRecord>,
}

/// Keeps ordinary diagnostics from becoming a global world-resource oracle.
impl Debug for SurfaceResourceState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SurfaceResourceState")
            .finish_non_exhaustive()
    }
}

/// Serializes complete surface truth only through the trusted root persistence boundary.
pub(crate) fn serialize_surface_resource_state<S>(
    state: &SurfaceResourceState,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    #[derive(Serialize)]
    struct PersistentSurfaceResourceState<'state> {
        revision: u64,
        next_resource_id: u32,
        resources: &'state BTreeMap<SurfaceResourceId, SurfaceResourceRecord>,
    }

    PersistentSurfaceResourceState {
        revision: state.revision,
        next_resource_id: state.next_resource_id,
        resources: &state.resources,
    }
    .serialize(serializer)
}

impl SurfaceResourceState {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            revision: 0,
            next_resource_id: 1,
            resources: BTreeMap::new(),
        }
    }

    #[must_use]
    pub(crate) const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub(crate) fn get(&self, id: SurfaceResourceId) -> Option<&SurfaceResourceRecord> {
        self.resources.get(&id)
    }

    /// Iterates currently gatherable loose resources at the supplied trusted observer voxel.
    pub(crate) fn available_resources_at(
        &self,
        position: Option<VoxelCoord>,
    ) -> impl Iterator<Item = &SurfaceResourceRecord> {
        self.resources.values().filter(move |resource| {
            Some(resource.position) == position
                && resource.lifecycle == SurfaceResourceLifecycle::Available
                && !resource.remaining_mass.is_zero()
        })
    }

    pub(crate) fn resources(&self) -> impl Iterator<Item = &SurfaceResourceRecord> {
        self.resources.values()
    }

    pub(crate) fn has_valid_id_cursor(&self) -> bool {
        self.next_resource_id != 0
            && self
                .resources
                .keys()
                .next_back()
                .is_none_or(|highest| highest.value() < self.next_resource_id)
    }

    #[cfg(any(test, feature = "test-gameplay"))]
    const fn next_resource_id(&self) -> u32 {
        self.next_resource_id
    }

    #[cfg(any(test, feature = "test-gameplay"))]
    fn insert_generated(
        &mut self,
        record: SurfaceResourceRecord,
        next_resource_id: u32,
        next_revision: u64,
    ) {
        assert_eq!(record.id.value(), self.next_resource_id);
        assert_eq!(self.next_resource_id.checked_add(1), Some(next_resource_id));
        assert_eq!(self.revision.checked_add(1), Some(next_revision));
        assert!(self.resources.insert(record.id, record).is_none());
        self.next_resource_id = next_resource_id;
        self.revision = next_revision;
    }

    pub(crate) fn apply_gathering(
        &mut self,
        resource: SurfaceResourceId,
        gathered: Mass,
        gathered_at: SimulationTick,
        next_revision: u64,
    ) {
        assert_eq!(self.revision.checked_add(1), Some(next_revision));
        let record = self.resources.get_mut(&resource).unwrap_or_else(|| {
            panic!("validated surface resource disappeared without revision change")
        });
        assert_eq!(record.lifecycle, SurfaceResourceLifecycle::Available);
        assert!(record.depleted_at.is_none());
        assert!(gathered_at >= record.generated_at);
        assert!(!gathered.is_zero());
        let remaining = record
            .remaining_mass
            .checked_sub(gathered)
            .unwrap_or_else(|| panic!("surface gathering exceeded remaining resource mass"));
        record.remaining_mass = remaining;
        if remaining.is_zero() {
            record.lifecycle = SurfaceResourceLifecycle::Depleted;
            record.depleted_at = Some(gathered_at);
        }
        self.revision = next_revision;
    }
}

#[cfg(any(test, feature = "test-gameplay"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GeneratedSurfaceResourceSpec {
    position: VoxelCoord,
    commodity: CommodityKey,
    mass: Mass,
    temperature: Temperature,
    composition: MaterialComposition,
}

#[cfg(any(test, feature = "test-gameplay"))]
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

#[cfg(any(test, feature = "test-gameplay"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GeneratedSurfaceResourceSpecError {
    ZeroMass,
    InvalidComposition(CompositionError),
    MissingHostMaterial { host: MaterialId },
}

#[cfg(any(test, feature = "test-gameplay"))]
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

#[cfg(any(test, feature = "test-gameplay"))]
impl Error for GeneratedSurfaceResourceSpecError {}

#[cfg(any(test, feature = "test-gameplay"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum InsertGeneratedSurfaceResourceError {
    InvalidState(SurfaceResourceValidationError),
    IdExhausted,
    RevisionExhausted,
}

#[cfg(any(test, feature = "test-gameplay"))]
impl Display for InsertGeneratedSurfaceResourceError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

#[cfg(any(test, feature = "test-gameplay"))]
impl Error for InsertGeneratedSurfaceResourceError {}

#[cfg(any(test, feature = "test-gameplay"))]
pub(crate) fn insert_generated_surface_resource(
    registries: &Registries,
    state: &mut AppState,
    spec: GeneratedSurfaceResourceSpec,
) -> Result<SurfaceResourceId, InsertGeneratedSurfaceResourceError> {
    let id = SurfaceResourceId::new(state.surface().next_resource_id());
    let next_id = state
        .surface()
        .next_resource_id()
        .checked_add(1)
        .ok_or(InsertGeneratedSurfaceResourceError::IdExhausted)?;
    let next_revision = state
        .surface()
        .revision()
        .checked_add(1)
        .ok_or(InsertGeneratedSurfaceResourceError::RevisionExhausted)?;
    let record = SurfaceResourceRecord {
        id,
        position: spec.position,
        commodity: spec.commodity,
        initial_mass: spec.mass,
        remaining_mass: spec.mass,
        temperature: spec.temperature,
        composition: spec.composition,
        lifecycle: SurfaceResourceLifecycle::Available,
        generated_at: state.tick(),
        depleted_at: None,
    };
    validation::validate_surface_resource_record(registries.materials(), id, &record, state.tick())
        .map_err(InsertGeneratedSurfaceResourceError::InvalidState)?;
    state
        .surface_state_mut()
        .insert_generated(record, next_id, next_revision);
    Ok(id)
}

mod validation;

pub use validation::SurfaceResourceValidationError;
pub(crate) use validation::validate_loaded_surface_resources;
