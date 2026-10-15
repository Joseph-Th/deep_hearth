//! Geometry-derived material requirement for controlled structural materialization.

#[cfg(any(test, feature = "test-gameplay"))]
use std::error::Error;
#[cfg(any(test, feature = "test-gameplay"))]
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
#[cfg(any(test, feature = "test-gameplay"))]
use crate::core::state::AppState;
#[cfg(any(test, feature = "test-gameplay"))]
use crate::material::MaterialId;
use crate::material::MaterialRegistry;
#[cfg(any(test, feature = "test-gameplay"))]
use crate::registry::Registries;

use super::super::geometry::{StructuralGeometryError, calculate_prismatic_material_mass_ceiling};
#[cfg(any(test, feature = "test-gameplay"))]
use super::super::state::StructuralElementId;
use super::super::state::StructuralElementRecord;

/// Read-only physical material requirement for one prismatic structural member.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg(any(test, feature = "test-gameplay"))]
pub(crate) struct StructuralMaterialRequirement {
    element: StructuralElementId,
    material: MaterialId,
    required_mass: Mass,
}

#[cfg(any(test, feature = "test-gameplay"))]
impl StructuralMaterialRequirement {
    #[must_use]
    #[cfg(test)]
    pub(crate) const fn element(self) -> StructuralElementId {
        self.element
    }

    #[must_use]
    #[cfg(any(test, feature = "test-gameplay"))]
    pub(crate) const fn material(self) -> MaterialId {
        self.material
    }

    #[must_use]
    pub(crate) const fn required_mass(self) -> Mass {
        self.required_mass
    }
}

/// Failure while deriving a member's physical solid-material requirement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg(any(test, feature = "test-gameplay"))]
pub(crate) enum StructuralMaterialRequirementError {
    UnknownElement {
        element: StructuralElementId,
    },
    Geometry {
        element: StructuralElementId,
        error: StructuralGeometryError,
    },
}

#[cfg(any(test, feature = "test-gameplay"))]
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

#[cfg(any(test, feature = "test-gameplay"))]
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

pub(crate) fn resolve_required_mass(
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
#[cfg(any(test, feature = "test-gameplay"))]
pub(crate) fn resolve_structural_material_requirement(
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
