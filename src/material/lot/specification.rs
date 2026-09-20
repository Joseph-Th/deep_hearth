//! Homogeneous runtime material-lot creation specifications.

use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Deserializer, Serialize};

use super::super::{
    CommodityKey, CompositionError, MaterialComposition, MaterialId, ParticleSizeDistribution,
    ParticleSizeRange,
};
use crate::core::quantity::{Mass, Temperature};

/// Specification for creating one homogeneous runtime material lot.
///
/// This is a boundary value shared by systems that produce matter. It is not a runtime record and
/// carries no owner or persistent lot ID; the inventory owner binds persistent identity during
/// canonical transaction planning and realizes it during commit.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct MaterialLotSpec {
    commodity: CommodityKey,
    mass: Mass,
    temperature: Temperature,
    composition: MaterialComposition,
    particle_size: Option<ParticleSizeDistribution>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MaterialLotSpecRepresentation {
    commodity: CommodityKey,
    mass: Mass,
    temperature: Temperature,
    composition: MaterialComposition,
    particle_size: Option<ParticleSizeDistribution>,
}

impl<'de> Deserialize<'de> for MaterialLotSpec {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let representation = MaterialLotSpecRepresentation::deserialize(deserializer)?;
        match representation.particle_size {
            Some(particle_size) => Self::with_composition_and_particle_size(
                representation.commodity,
                representation.mass,
                representation.temperature,
                representation.composition,
                particle_size,
            ),
            None => Self::with_composition(
                representation.commodity,
                representation.mass,
                representation.temperature,
                representation.composition,
            ),
        }
        .map_err(serde::de::Error::custom)
    }
}

impl MaterialLotSpec {
    #[must_use]
    pub fn new(commodity: CommodityKey, mass: Mass, temperature: Temperature) -> Self {
        assert!(
            !mass.is_zero(),
            "material lot specification mass must be nonzero"
        );
        Self {
            commodity,
            mass,
            temperature,
            composition: MaterialComposition::pure(commodity.material()),
            particle_size: None,
        }
    }

    /// Builds a lot specification with an explicit normalized composition.
    pub fn with_composition(
        commodity: CommodityKey,
        mass: Mass,
        temperature: Temperature,
        composition: MaterialComposition,
    ) -> Result<Self, MaterialLotSpecError> {
        if mass.is_zero() {
            return Err(MaterialLotSpecError::ZeroMass);
        }
        composition
            .validate()
            .map_err(MaterialLotSpecError::InvalidComposition)?;
        if composition.parts_per_million(commodity.material()) == 0 {
            return Err(MaterialLotSpecError::MissingHostMaterial {
                host: commodity.material(),
            });
        }
        Ok(Self {
            commodity,
            mass,
            temperature,
            composition,
            particle_size: None,
        })
    }

    /// Builds a lot specification with explicit composition and particulate size information.
    pub fn with_composition_and_particle_size<P>(
        commodity: CommodityKey,
        mass: Mass,
        temperature: Temperature,
        composition: MaterialComposition,
        particle_size: P,
    ) -> Result<Self, MaterialLotSpecError>
    where
        P: Into<ParticleSizeDistribution>,
    {
        let mut specification = Self::with_composition(commodity, mass, temperature, composition)?;
        specification.particle_size = Some(particle_size.into());
        Ok(specification)
    }

    #[must_use]
    pub const fn commodity(&self) -> CommodityKey {
        self.commodity
    }

    #[must_use]
    pub const fn mass(&self) -> Mass {
        self.mass
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
    pub fn particle_size(&self) -> Option<ParticleSizeRange> {
        self.particle_size
            .as_ref()
            .map(ParticleSizeDistribution::envelope)
    }

    /// Returns the authoritative weighted particulate profile, if this form tracks one.
    #[must_use]
    pub const fn particle_size_distribution(&self) -> Option<&ParticleSizeDistribution> {
        self.particle_size.as_ref()
    }
}

/// Construction failure for a material lot specification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MaterialLotSpecError {
    ZeroMass,
    InvalidComposition(CompositionError),
    MissingHostMaterial { host: MaterialId },
}

impl Display for MaterialLotSpecError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroMass => {
                formatter.write_str("material lot specification mass must be nonzero")
            }
            Self::InvalidComposition(error) => {
                write!(formatter, "invalid lot composition: {error}")
            }
            Self::MissingHostMaterial { host } => write!(
                formatter,
                "material lot composition does not contain host material {}",
                host.value()
            ),
        }
    }
}

impl Error for MaterialLotSpecError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidComposition(error) => Some(error),
            Self::ZeroMass => None,
            Self::MissingHostMaterial { host: _host } => None,
        }
    }
}
