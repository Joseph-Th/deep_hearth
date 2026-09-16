//! Geometry-derived material requirement for controlled structural materialization.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::material::{MaterialId, MaterialRegistry};
use crate::registry::Registries;

use super::super::geometry::{StructuralGeometryError, calculate_prismatic_material_mass_ceiling};
use super::super::state::{StructuralElementId, StructuralElementRecord};

/// Read-only physical material requirement for one prismatic structural member.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StructuralMaterialRequirement {
    element: StructuralElementId,
    material: MaterialId,
    required_mass: Mass,
}

impl StructuralMaterialRequirement {
    #[must_use]
    #[cfg(test)]
    pub(crate) const fn element(self) -> StructuralElementId {
        self.element
    }

    #[must_use]
    pub const fn material(self) -> MaterialId {
        self.material
    }

    #[must_use]
    pub const fn required_mass(self) -> Mass {
        self.required_mass
    }
}

/// Failure while deriving a member's physical solid-material requirement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StructuralMaterialRequirementError {
    UnknownElement {
        element: StructuralElementId,
    },
    Geometry {
        element: StructuralElementId,
        error: StructuralGeometryError,
    },
}

impl Display for StructuralMaterialRequirementError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownElement { element } => {
                write!(formatter, "unknown structural element {}", element.value())
            }
            Self::Geometry { element, error } => write!(
                formatter,
                "structural element {} material requirement cannot be resolved: {error}",
                element.value()
            ),
        }
    }
}

impl Error for StructuralMaterialRequirementError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Geometry {
                element: _element,
                error,
            } => Some(error),
            Self::UnknownElement { element: _element } => None,
        }
    }
}

pub(super) fn resolve_required_mass(
    materials: &MaterialRegistry,
    record: &StructuralElementRecord,
) -> Result<Mass, StructuralGeometryError> {
    calculate_prismatic_material_mass_ceiling(
        materials,
        record.material(),
        record.cross_section(),
        record.length(),
    )
}

/// Derives conservative solid volume and exact milligram ownership from member geometry and density.
pub fn resolve_structural_material_requirement(
    registries: &Registries,
    state: &AppState,
    element: StructuralElementId,
) -> Result<StructuralMaterialRequirement, StructuralMaterialRequirementError> {
    let record = state
        .structures()
        .get_element(element)
        .ok_or(StructuralMaterialRequirementError::UnknownElement { element })?;
    let required_mass = resolve_required_mass(registries.materials(), record)
        .map_err(|error| StructuralMaterialRequirementError::Geometry { element, error })?;
    Ok(StructuralMaterialRequirement {
        element,
        material: record.material(),
        required_mass,
    })
}
