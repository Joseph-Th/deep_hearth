//! Pure-material latent heat at the authored solid/liquid fusion boundary.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Energy, Mass, Temperature};
use crate::material::{MaterialId, MaterialRegistry};

/// Exact latent-energy requirement for melting one pure material mass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FusionHeat {
    energy: Energy,
    melting_point: Temperature,
}

impl FusionHeat {
    #[must_use]
    pub const fn energy(self) -> Energy {
        self.energy
    }

    #[must_use]
    pub const fn melting_point(self) -> Temperature {
        self.melting_point
    }
}

/// Failure to resolve latent heat from authored material properties.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FusionHeatError {
    UnknownMaterial { material: MaterialId },
    MissingFusionProperties { material: MaterialId },
    ArithmeticOverflow,
}

impl Display for FusionHeatError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownMaterial { material } => {
                write!(
                    formatter,
                    "unknown material {} in fusion calculation",
                    material.value()
                )
            }
            Self::MissingFusionProperties { material } => write!(
                formatter,
                "material {} has no authored solid/liquid fusion properties",
                material.value()
            ),
            Self::ArithmeticOverflow => formatter
                .write_str("fusion latent-heat calculation overflowed authoritative energy"),
        }
    }
}

impl Error for FusionHeatError {}

/// Calculates exact latent heat for melting a pure material mass at its authored fusion boundary.
pub fn calculate_fusion_heat(
    materials: &MaterialRegistry,
    mass: Mass,
    material: MaterialId,
) -> Result<FusionHeat, FusionHeatError> {
    let Some(definition) = materials.get_material(material) else {
        return Err(FusionHeatError::UnknownMaterial { material });
    };
    let Some(fusion) = definition.properties().thermal().fusion() else {
        return Err(FusionHeatError::MissingFusionProperties { material });
    };
    let nanojoules = u128::from(mass.milligrams())
        .checked_mul(u128::from(fusion.latent_heat_j_per_kg()))
        .and_then(|value| value.checked_mul(1_000))
        .ok_or(FusionHeatError::ArithmeticOverflow)?;
    Ok(FusionHeat {
        energy: Energy::from_nanojoules(nanojoules),
        melting_point: fusion.melting_point(),
    })
}
