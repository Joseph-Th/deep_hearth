//! Validated creation of finite loose world matter in surface ownership.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Mass, Temperature};
use crate::core::state::AppState;
use crate::core::time::{ControlTick, SimulationTick};
use crate::inventory::{MaterialLotProfile, MaterialLotProvenance};
use crate::material::{
    CommodityKey, CompositionError, MaterialComposition, MaterialId, MaterialLotSpec,
    MaterialLotSpecError,
};
use crate::registry::Registries;
use crate::spatial::VoxelCoord;

use super::state::validation::{SurfaceResourceValidationError, validate_surface_resource_record};
use super::state::{
    SurfaceMaterialParcel, SurfaceResourceId, SurfaceResourceKind, SurfaceResourceRecord,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SurfaceResourceSpec {
    kind: SurfaceResourceKind,
    position: VoxelCoord,
    mass: Mass,
    profile: MaterialLotProfile,
    provenance: Option<MaterialLotProvenance>,
    ambient_age_parts_at_created_at: u128,
    automatic_pickup_delay_started_at: Option<ControlTick>,
}

impl SurfaceResourceSpec {
    pub(crate) fn new(
        kind: SurfaceResourceKind,
        position: VoxelCoord,
        commodity: CommodityKey,
        mass: Mass,
        temperature: Temperature,
        composition: MaterialComposition,
    ) -> Result<Self, SurfaceResourceSpecError> {
        let material = MaterialLotSpec::with_composition(commodity, mass, temperature, composition)
            .map_err(|error| match error {
                MaterialLotSpecError::ZeroMass => SurfaceResourceSpecError::ZeroMass,
                MaterialLotSpecError::InvalidComposition(error) => {
                    SurfaceResourceSpecError::InvalidComposition(error)
                }
                MaterialLotSpecError::MissingHostMaterial { host } => {
                    SurfaceResourceSpecError::MissingHostMaterial { host }
                }
            })?;
        Ok(Self {
            kind,
            position,
            mass: material.mass(),
            profile: MaterialLotProfile::from_spec(&material),
            provenance: None,
            ambient_age_parts_at_created_at: 0,
            automatic_pickup_delay_started_at: None,
        })
    }

    pub(crate) fn from_existing_material(
        kind: SurfaceResourceKind,
        position: VoxelCoord,
        mass: Mass,
        profile: MaterialLotProfile,
        provenance: MaterialLotProvenance,
        ambient_age_parts_at_created_at: u128,
    ) -> Result<Self, SurfaceResourceSpecError> {
        if mass.is_zero() {
            return Err(SurfaceResourceSpecError::ZeroMass);
        }
        Ok(Self {
            kind,
            position,
            mass,
            profile,
            provenance: Some(provenance),
            ambient_age_parts_at_created_at,
            automatic_pickup_delay_started_at: None,
        })
    }

    #[must_use]
    pub(crate) const fn with_automatic_pickup_delay_started_at(
        mut self,
        tick: ControlTick,
    ) -> Self {
        self.automatic_pickup_delay_started_at = Some(tick);
        self
    }

    fn into_parcel(self, created_at: SimulationTick) -> SurfaceMaterialParcel {
        SurfaceMaterialParcel::existing(
            self.mass,
            self.profile,
            self.provenance
                .unwrap_or_else(|| MaterialLotProvenance::single(created_at)),
            self.ambient_age_parts_at_created_at,
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SurfaceResourceSpecError {
    ZeroMass,
    InvalidComposition(CompositionError),
    MissingHostMaterial { host: MaterialId },
}

impl Display for SurfaceResourceSpecError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroMass => formatter.write_str("surface resource mass must be nonzero"),
            Self::InvalidComposition(error) => {
                write!(formatter, "invalid surface resource composition: {error}")
            }
            Self::MissingHostMaterial { host } => write!(
                formatter,
                "surface resource composition omits host material {}",
                host.value()
            ),
        }
    }
}

impl Error for SurfaceResourceSpecError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidComposition(error) => Some(error),
            Self::ZeroMass | Self::MissingHostMaterial { .. } => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SurfaceResourceCreationError {
    Empty,
    IdExhausted,
    RevisionExhausted,
    InvalidResource(SurfaceResourceValidationError),
}

impl Display for SurfaceResourceCreationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => formatter.write_str("surface resource creation must contain a resource"),
            Self::IdExhausted => {
                formatter.write_str("surface resource identifier space is exhausted")
            }
            Self::RevisionExhausted => {
                formatter.write_str("surface resource revision space is exhausted")
            }
            Self::InvalidResource(error) => {
                write!(formatter, "created surface resource is invalid: {error}")
            }
        }
    }
}

impl Error for SurfaceResourceCreationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidResource(error) => Some(error),
            Self::Empty | Self::IdExhausted | Self::RevisionExhausted => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SurfaceResourceCreationCommitError {
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

impl Display for SurfaceResourceCreationCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Tick { expected, actual } => write!(
                formatter,
                "surface resource creation expected tick {} but state advanced to {} before commit",
                expected.value(),
                actual.value()
            ),
            Self::Revision { expected, actual } => write!(
                formatter,
                "surface resource revision changed from {expected} to {actual} before creation commit"
            ),
            Self::NextResourceId { expected, actual } => write!(
                formatter,
                "surface resource creation expected next id {expected} but found {actual} before commit"
            ),
        }
    }
}

impl Error for SurfaceResourceCreationCommitError {}

#[must_use]
pub(crate) struct ValidatedSurfaceResourceCreation {
    expected_tick: SimulationTick,
    expected_revision: u64,
    expected_next_resource_id: u32,
    records: Vec<SurfaceResourceRecord>,
    next_resource_id: u32,
    next_revision: u64,
}

impl ValidatedSurfaceResourceCreation {
    pub(crate) fn precheck(
        &self,
        state: &AppState,
    ) -> Result<(), SurfaceResourceCreationCommitError> {
        let actual_tick = state.tick();
        if actual_tick != self.expected_tick {
            return Err(SurfaceResourceCreationCommitError::Tick {
                expected: self.expected_tick,
                actual: actual_tick,
            });
        }
        let actual_revision = state.surface().revision();
        if actual_revision != self.expected_revision {
            return Err(SurfaceResourceCreationCommitError::Revision {
                expected: self.expected_revision,
                actual: actual_revision,
            });
        }
        let actual_next_resource_id = state.surface().next_resource_id();
        if actual_next_resource_id != self.expected_next_resource_id {
            return Err(SurfaceResourceCreationCommitError::NextResourceId {
                expected: self.expected_next_resource_id,
                actual: actual_next_resource_id,
            });
        }
        Ok(())
    }

    #[must_use]
    pub(crate) fn first_resource_id(&self) -> SurfaceResourceId {
        self.records
            .first()
            .map(SurfaceResourceRecord::id)
            .unwrap_or_else(|| unreachable!("validated surface creation contains a resource"))
    }

    pub(crate) fn resource_ids(&self) -> impl ExactSizeIterator<Item = SurfaceResourceId> + '_ {
        self.records.iter().map(SurfaceResourceRecord::id)
    }

    pub(crate) fn apply(self, state: &mut AppState) {
        state.surface_state_mut().apply_created_batch(
            self.records,
            self.next_resource_id,
            self.next_revision,
        );
    }

    #[cfg(any(test, feature = "test-gameplay"))]
    pub(crate) fn commit(
        self,
        state: &mut AppState,
    ) -> Result<(), SurfaceResourceCreationCommitError> {
        self.precheck(state)?;
        self.apply(state);
        Ok(())
    }
}

pub(crate) fn validate_create_surface_resources(
    registries: &Registries,
    state: &AppState,
    specs: impl IntoIterator<Item = SurfaceResourceSpec>,
) -> Result<ValidatedSurfaceResourceCreation, SurfaceResourceCreationError> {
    validate_create_surface_resources_at(registries, state, specs, state.tick())
}

pub(crate) fn validate_create_surface_resources_at(
    registries: &Registries,
    state: &AppState,
    specs: impl IntoIterator<Item = SurfaceResourceSpec>,
    created_at: SimulationTick,
) -> Result<ValidatedSurfaceResourceCreation, SurfaceResourceCreationError> {
    let specs = specs.into_iter().collect::<Vec<_>>();
    if specs.is_empty() {
        return Err(SurfaceResourceCreationError::Empty);
    }
    let expected_revision = state.surface().revision();
    let expected_next_resource_id = state.surface().next_resource_id();
    let next_revision = expected_revision
        .checked_add(1)
        .ok_or(SurfaceResourceCreationError::RevisionExhausted)?;
    let mut next_resource_id = expected_next_resource_id;
    let mut records = Vec::with_capacity(specs.len());
    for spec in specs {
        let id = SurfaceResourceId::new(next_resource_id);
        next_resource_id = next_resource_id
            .checked_add(1)
            .ok_or(SurfaceResourceCreationError::IdExhausted)?;
        let kind = spec.kind;
        let position = spec.position;
        let automatic_pickup_delay_started_at = spec.automatic_pickup_delay_started_at;
        let parcel = spec.into_parcel(created_at);
        let record = SurfaceResourceRecord::created(
            id,
            kind,
            position,
            parcel,
            created_at,
            automatic_pickup_delay_started_at,
        );
        validate_surface_resource_record(
            registries.materials(),
            id,
            &record,
            created_at,
            state.control_tick(),
        )
        .map_err(SurfaceResourceCreationError::InvalidResource)?;
        records.push(record);
    }
    Ok(ValidatedSurfaceResourceCreation {
        expected_tick: state.tick(),
        expected_revision,
        expected_next_resource_id,
        records,
        next_resource_id,
        next_revision,
    })
}

#[cfg(test)]
pub(crate) fn insert_surface_resource_for_test(
    registries: &Registries,
    state: &mut AppState,
    spec: SurfaceResourceSpec,
) -> Result<SurfaceResourceId, SurfaceResourceCreationError> {
    let id = SurfaceResourceId::new(state.surface().next_resource_id());
    validate_create_surface_resources(registries, state, [spec])?
        .commit(state)
        .unwrap_or_else(|error| {
            unreachable!("immediate test surface creation cannot become stale: {error}")
        });
    Ok(id)
}
