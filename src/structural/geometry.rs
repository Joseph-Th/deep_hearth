//! Exact prismatic member geometry and density-derived material quantity.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Area, Length, Mass, Volume};
use crate::material::{MaterialId, MaterialRegistry};

/// Failure while projecting exact structural geometry into authoritative physical quantities.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StructuralGeometryError {
    ZeroCrossSection,
    ZeroLength,
    UnknownMaterial { material: MaterialId },
    ArithmeticOverflow,
    VolumeOutOfRange,
    MassOutOfRange,
}

impl Display for StructuralGeometryError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroCrossSection => {
                formatter.write_str("structural member cross-sectional area must be nonzero")
            }
            Self::ZeroLength => formatter.write_str("structural member length must be nonzero"),
            Self::UnknownMaterial { material } => write!(
                formatter,
                "structural geometry references unknown material {}",
                material.value()
            ),
            Self::ArithmeticOverflow => formatter
                .write_str("structural geometry calculation overflowed intermediate storage"),
            Self::VolumeOutOfRange => {
                formatter.write_str("structural solid volume exceeds authoritative volume range")
            }
            Self::MassOutOfRange => {
                formatter.write_str("structural material mass exceeds authoritative mass range")
            }
        }
    }
}

impl Error for StructuralGeometryError {}

/// Returns conservative solid volume for a prismatic member.
///
/// Cross-section is stored in square millimeters and length in micrometers. Since one microliter is
/// one cubic millimeter, `volume_uL = area_mm2 * length_um / 1000`. The result rounds upward so a
/// sub-microliter solid is never erased at the authoritative volume boundary.
pub fn calculate_prismatic_volume_ceiling(
    cross_section: Area,
    length: Length,
) -> Result<Volume, StructuralGeometryError> {
    validate_dimensions(cross_section, length)?;
    let numerator = u128::from(cross_section.square_millimeters())
        .checked_mul(u128::from(length.micrometers()))
        .ok_or(StructuralGeometryError::ArithmeticOverflow)?;
    let microliters = numerator.div_ceil(1_000);
    let microliters =
        u64::try_from(microliters).map_err(|_| StructuralGeometryError::VolumeOutOfRange)?;
    Ok(Volume::from_microliters(microliters))
}

/// Returns conservative pure-material mass for a prismatic member directly from exact geometry.
///
/// The calculation deliberately does not feed the rounded volume result back into mass. Combining
/// the unit conversions gives `mass_mg = area_mm2 * length_um * density_kg_m3 / 1_000_000`, rounded
/// upward once at the milligram ownership boundary. This avoids compounding volume quantization for
/// dense materials while still never creating a mass deficit from rounding down.
pub fn calculate_prismatic_material_mass_ceiling(
    materials: &MaterialRegistry,
    material: MaterialId,
    cross_section: Area,
    length: Length,
) -> Result<Mass, StructuralGeometryError> {
    validate_dimensions(cross_section, length)?;
    let definition = materials
        .get_material(material)
        .ok_or(StructuralGeometryError::UnknownMaterial { material })?;
    let numerator = u128::from(cross_section.square_millimeters())
        .checked_mul(u128::from(length.micrometers()))
        .and_then(|value| {
            value.checked_mul(u128::from(definition.properties().density_kg_per_m3()))
        })
        .ok_or(StructuralGeometryError::ArithmeticOverflow)?;
    let milligrams = numerator.div_ceil(1_000_000);
    let milligrams =
        u64::try_from(milligrams).map_err(|_| StructuralGeometryError::MassOutOfRange)?;
    Ok(Mass::from_milligrams(milligrams))
}

fn validate_dimensions(cross_section: Area, length: Length) -> Result<(), StructuralGeometryError> {
    if cross_section.is_zero() {
        return Err(StructuralGeometryError::ZeroCrossSection);
    }
    if length.is_zero() {
        return Err(StructuralGeometryError::ZeroLength);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{MATERIAL_COPPER, MATERIAL_WOOD, build_registries};

    #[test]
    fn prismatic_volume_preserves_sub_millimeter_length() {
        assert_eq!(
            calculate_prismatic_volume_ceiling(
                Area::from_square_millimeters(1_000),
                Length::from_micrometers(1),
            ),
            Ok(Volume::from_microliters(1))
        );
    }

    #[test]
    fn material_mass_uses_density_without_compounded_volume_rounding() {
        let registries = build_registries();
        let area = Area::from_square_millimeters(1);
        let length = Length::from_micrometers(1);

        assert_eq!(
            calculate_prismatic_material_mass_ceiling(
                registries.materials(),
                MATERIAL_WOOD,
                area,
                length,
            ),
            Ok(Mass::from_milligrams(1))
        );
        assert_eq!(
            calculate_prismatic_material_mass_ceiling(
                registries.materials(),
                MATERIAL_COPPER,
                area,
                length,
            ),
            Ok(Mass::from_milligrams(1))
        );
    }

    #[test]
    fn equal_geometry_requires_more_dense_material_mass() {
        let registries = build_registries();
        let area = Area::from_square_millimeters(1_000);
        let length = Length::from_micrometers(10_000);
        let wood = calculate_prismatic_material_mass_ceiling(
            registries.materials(),
            MATERIAL_WOOD,
            area,
            length,
        );
        let copper = calculate_prismatic_material_mass_ceiling(
            registries.materials(),
            MATERIAL_COPPER,
            area,
            length,
        );

        assert_eq!(wood, Ok(Mass::from_milligrams(6_500)));
        assert_eq!(copper, Ok(Mass::from_milligrams(89_600)));
    }
}
