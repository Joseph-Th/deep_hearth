//! Read-only physical projection for one prismatic structural member under external load.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Area, Force, Length, Mass};
use crate::material::MaterialId;
use crate::registry::Registries;

use super::analysis::{
    calculate_pristine_member_capacity, calculate_structural_utilization_ppm,
    is_structural_load_within_utilization_limit,
};
use super::definitions::StructuralProfileId;
use super::geometry::{StructuralGeometryError, calculate_prismatic_material_mass_ceiling};
use super::load::calculate_weight_force_ceiling;

/// Physical inputs for one pristine prismatic-member load projection.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrismaticMemberLoadRequest {
    profile: StructuralProfileId,
    material: MaterialId,
    length: Length,
    cross_section: Area,
    external_load: Force,
}

impl PrismaticMemberLoadRequest {
    pub const fn new(
        profile: StructuralProfileId,
        material: MaterialId,
        length: Length,
        cross_section: Area,
        external_load: Force,
    ) -> Self {
        Self {
            profile,
            material,
            length,
            cross_section,
            external_load,
        }
    }
}

/// Canonical read-only load projection for one pristine prismatic member.
///
/// This combines the member's density-derived mass and self-weight with a caller-supplied external
/// load, then resolves pristine capacity and utilization through the structural owner's existing
/// physical rules. It does not inspect support topology, persistent damage, or runtime ownership.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrismaticMemberLoadProjection {
    material_mass: Mass,
    self_weight: Force,
    total_load: Force,
    pristine_capacity: Force,
    utilization_ppm: u128,
}

impl PrismaticMemberLoadProjection {
    #[must_use]
    pub const fn material_mass(self) -> Mass {
        self.material_mass
    }

    #[must_use]
    pub const fn self_weight(self) -> Force {
        self.self_weight
    }

    #[must_use]
    pub const fn total_load(self) -> Force {
        self.total_load
    }

    #[must_use]
    pub const fn pristine_capacity(self) -> Force {
        self.pristine_capacity
    }

    #[must_use]
    pub const fn utilization_ppm(self) -> u128 {
        self.utilization_ppm
    }

    /// Reports exact compliance with an authored normalized utilization limit.
    #[must_use]
    pub fn is_within_utilization_limit(self, maximum_ppm: u32) -> bool {
        is_structural_load_within_utilization_limit(
            self.total_load,
            self.pristine_capacity,
            maximum_ppm,
        )
    }
}

/// Failure while projecting a pristine prismatic member under an external load.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StructuralMemberLoadProjectionError {
    UnknownProfile { profile: StructuralProfileId },
    UnknownMaterial { material: MaterialId },
    NonStructuralMaterial { material: MaterialId },
    Geometry(StructuralGeometryError),
    LoadOverflow,
}

impl Display for StructuralMemberLoadProjectionError {
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
            Self::NonStructuralMaterial { material } => write!(
                formatter,
                "material {} has no structural strength properties",
                material.value()
            ),
            Self::Geometry(error) => {
                write!(
                    formatter,
                    "structural member geometry cannot be projected: {error}"
                )
            }
            Self::LoadOverflow => {
                formatter.write_str("structural member total load exceeds representable force")
            }
        }
    }
}

impl Error for StructuralMemberLoadProjectionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Geometry(error) => Some(error),
            Self::UnknownProfile { .. }
            | Self::UnknownMaterial { .. }
            | Self::NonStructuralMaterial { .. }
            | Self::LoadOverflow => None,
        }
    }
}

/// Projects one pristine prismatic member using canonical geometry, gravity, and strength rules.
pub fn project_prismatic_member_load(
    registries: &Registries,
    request: PrismaticMemberLoadRequest,
) -> Result<PrismaticMemberLoadProjection, StructuralMemberLoadProjectionError> {
    let PrismaticMemberLoadRequest {
        profile: profile_id,
        material: material_id,
        length,
        cross_section,
        external_load,
    } = request;
    let profile = registries.structural().get_profile(profile_id).ok_or(
        StructuralMemberLoadProjectionError::UnknownProfile {
            profile: profile_id,
        },
    )?;
    let material = registries.materials().get_material(material_id).ok_or(
        StructuralMemberLoadProjectionError::UnknownMaterial {
            material: material_id,
        },
    )?;
    let material_mass = calculate_prismatic_material_mass_ceiling(
        registries.materials(),
        material_id,
        cross_section,
        length,
    )
    .map_err(StructuralMemberLoadProjectionError::Geometry)?;
    let self_weight = calculate_weight_force_ceiling(material_mass, registries.core().gravity());
    let total_load = Force::from_millinewtons(
        external_load
            .millinewtons()
            .checked_add(self_weight.millinewtons())
            .ok_or(StructuralMemberLoadProjectionError::LoadOverflow)?,
    );
    let pristine_capacity = calculate_pristine_member_capacity(profile, material, cross_section)
        .ok_or(StructuralMemberLoadProjectionError::NonStructuralMaterial {
            material: material_id,
        })?;
    let utilization_ppm = calculate_structural_utilization_ppm(total_load, pristine_capacity);
    Ok(PrismaticMemberLoadProjection {
        material_mass,
        self_weight,
        total_load,
        pristine_capacity,
        utilization_ppm,
    })
}

#[cfg(test)]
#[path = "member_projection_tests.rs"]
mod tests;
