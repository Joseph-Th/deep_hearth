//! Density-based material-volume calculation for sibling material definitions and runtime composition.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Mass, Volume};

use super::{MaterialComposition, MaterialId, MaterialRegistry};

/// Failure to resolve conservative physical volume from authoritative mass and composition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterialVolumeError {
    UnknownMaterial { material: MaterialId },
    ArithmeticOverflow,
    VolumeOutOfRange,
}

impl Display for MaterialVolumeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownMaterial { material } => write!(
                formatter,
                "material volume references unknown material {}",
                material.value()
            ),
            Self::ArithmeticOverflow => {
                formatter.write_str("material volume calculation overflowed intermediate storage")
            }
            Self::VolumeOutOfRange => {
                formatter.write_str("material volume exceeds authoritative volume range")
            }
        }
    }
}

impl Error for MaterialVolumeError {}

/// Calculates a conservative material volume from mass, normalized composition, and density.
///
/// Density is authored in kg/m^3, mass is stored in mg, and the result is in microliters. For each
/// constituent, `volume_uL = mass_mg * ppm / (1000 * density_kg_m3)`. Each partial constituent
/// contribution rounds upward before summation, so a capacity check using this value can slightly
/// overestimate volume but never gains storage space from rounding down.
pub fn calculate_volume_ceiling(
    materials: &MaterialRegistry,
    mass: Mass,
    composition: &MaterialComposition,
) -> Result<Volume, MaterialVolumeError> {
    let mut total_microliters = 0_u128;
    for component in composition.components() {
        let Some(definition) = materials.get_material(component.material()) else {
            return Err(MaterialVolumeError::UnknownMaterial {
                material: component.material(),
            });
        };
        let numerator = u128::from(mass.milligrams())
            .checked_mul(u128::from(component.parts_per_million()))
            .ok_or(MaterialVolumeError::ArithmeticOverflow)?;
        let denominator = u128::from(definition.properties().density_kg_per_m3())
            .checked_mul(1_000)
            .ok_or(MaterialVolumeError::ArithmeticOverflow)?;
        debug_assert!(
            denominator > 0,
            "material density invariant must be nonzero"
        );
        let rounded_up = numerator.div_ceil(denominator);
        total_microliters = total_microliters
            .checked_add(rounded_up)
            .ok_or(MaterialVolumeError::ArithmeticOverflow)?;
    }
    let microliters =
        u64::try_from(total_microliters).map_err(|_| MaterialVolumeError::VolumeOutOfRange)?;
    Ok(Volume::from_microliters(microliters))
}

#[cfg(test)]
#[path = "volume_tests.rs"]
mod tests;
