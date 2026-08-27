//! Canonical admission of inert planned structural members.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::state::AppState;
use crate::material::MaterialId;
use crate::registry::Registries;

use super::definitions::StructuralProfileId;
use super::geometry::StructuralGeometryError;
use super::state::{
    StructuralElementConfiguration, StructuralElementGeometry, StructuralElementId,
    StructuralElementRecord, StructuralLifecycle,
};

/// Failure while allocating a planned structural member and its synchronized indexes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddStructuralElementError {
    UnknownProfile { profile: StructuralProfileId },
    UnknownMaterial { material: MaterialId },
    Geometry(StructuralGeometryError),
    IdExhausted,
    RevisionExhausted,
}

impl Display for AddStructuralElementError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownProfile { profile } => {
                write!(formatter, "unknown structural profile {}", profile.value())
            }
            Self::UnknownMaterial { material } => {
                write!(
                    formatter,
                    "unknown structural material {}",
                    material.value()
                )
            }
            Self::Geometry(error) => write!(formatter, "invalid structural geometry: {error}"),
            Self::IdExhausted => {
                formatter.write_str("structural element identifier space is exhausted")
            }
            Self::RevisionExhausted => {
                formatter.write_str("structural state revision space is exhausted")
            }
        }
    }
}

impl Error for AddStructuralElementError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Geometry(error) => Some(error),
            Self::UnknownProfile { profile: _profile } => None,
            Self::UnknownMaterial { .. } => None,
            Self::IdExhausted | Self::RevisionExhausted => None,
        }
    }
}

/// Adds an inert planned member. It cannot carry or transmit load until activated canonically.
pub fn add_structural_element(
    registries: &Registries,
    state: &mut AppState,
    profile: StructuralProfileId,
    material: MaterialId,
    geometry: StructuralElementGeometry,
    is_grounded: bool,
) -> Result<StructuralElementId, AddStructuralElementError> {
    registries
        .structural()
        .get_profile(profile)
        .ok_or(AddStructuralElementError::UnknownProfile { profile })?;
    if registries.materials().get_material(material).is_none() {
        return Err(AddStructuralElementError::UnknownMaterial { material });
    }
    geometry
        .validate()
        .map_err(AddStructuralElementError::Geometry)?;
    let structures = state.structures();
    let id = StructuralElementId::new(structures.next_element_id());
    let next_element_id = structures
        .next_element_id()
        .checked_add(1)
        .ok_or(AddStructuralElementError::IdExhausted)?;
    let next_revision = structures
        .revision()
        .checked_add(1)
        .ok_or(AddStructuralElementError::RevisionExhausted)?;
    let record = StructuralElementRecord {
        id,
        configuration: StructuralElementConfiguration {
            profile,
            material,
            geometry,
            is_grounded,
        },
        embodied_mass: crate::core::quantity::Mass::ZERO,
        embodied_material: Vec::new(),
        loads: Default::default(),
        lifecycle: StructuralLifecycle::Planned,
        is_cracked: false,
        created_at: state.tick(),
    };

    state
        .structure_state_mut()
        .insert_element(record, next_element_id, next_revision);
    Ok(id)
}
