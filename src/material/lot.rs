//! Process input requirements and runtime material-lot creation specifications.

use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use super::{
    CommodityKey, CompositionConstraint, CompositionError, MaterialComposition, MaterialId,
    ParticleSizeDistribution, ParticleSizeRange,
};
use crate::core::quantity::{Mass, Temperature};

/// Matter requirement for a process, including optional composition ranges.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct MaterialInputSpec {
    commodity: CommodityKey,
    mass: Mass,
    constraints: Vec<CompositionConstraint>,
}

impl MaterialInputSpec {
    /// Builds an input requirement that accepts any composition with the requested host/form.
    #[must_use]
    pub fn new(commodity: CommodityKey, mass: Mass) -> Self {
        assert!(
            !mass.is_zero(),
            "material input specification mass must be nonzero"
        );
        Self {
            commodity,
            mass,
            constraints: Vec::new(),
        }
    }

    /// Builds a composition-constrained input requirement in canonical material-ID order.
    pub fn with_constraints(
        commodity: CommodityKey,
        mass: Mass,
        mut constraints: Vec<CompositionConstraint>,
    ) -> Result<Self, MaterialInputSpecError> {
        if mass.is_zero() {
            return Err(MaterialInputSpecError::ZeroMass);
        }
        constraints.sort_by_key(|constraint| constraint.material());
        for pair in constraints.windows(2) {
            if pair[0].material() == pair[1].material() {
                return Err(MaterialInputSpecError::DuplicateConstraint {
                    material: pair[0].material(),
                });
            }
        }
        Ok(Self {
            commodity,
            mass,
            constraints,
        })
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
    pub fn constraints(&self) -> &[CompositionConstraint] {
        &self.constraints
    }

    #[must_use]
    pub fn is_satisfied_by(&self, composition: &MaterialComposition) -> bool {
        self.constraints
            .iter()
            .all(|constraint| constraint.is_satisfied_by(composition))
    }
}

/// Construction failure for a composition-aware material input requirement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterialInputSpecError {
    ZeroMass,
    DuplicateConstraint { material: MaterialId },
}

impl Display for MaterialInputSpecError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroMass => {
                formatter.write_str("material input specification mass must be nonzero")
            }
            Self::DuplicateConstraint { material } => write!(
                formatter,
                "material input specification repeats constraint for material {}",
                material.value()
            ),
        }
    }
}

impl Error for MaterialInputSpecError {}

/// Specification for creating one homogeneous runtime material lot.
///
/// This is a boundary value shared by systems that produce matter. It is not a runtime record and
/// carries no owner or persistent lot ID; the inventory owner allocates those during canonical
/// commit.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MaterialLotSpec {
    commodity: CommodityKey,
    mass: Mass,
    temperature: Temperature,
    composition: MaterialComposition,
    particle_size: Option<ParticleSizeDistribution>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::material::{COMPOSITION_PARTS_PER_MILLION, CompositionComponent, FormId};

    #[test]
    fn input_spec_rejects_duplicate_material_constraints() {
        let material = MaterialId::new(3);
        let constraint = CompositionConstraint::new(material, 100_000, 900_000)
            .unwrap_or_else(|error| panic!("constraint fixture failed: {error}"));

        assert_eq!(
            MaterialInputSpec::with_constraints(
                CommodityKey::new(material, FormId::new(1)),
                Mass::from_milligrams(10),
                vec![constraint, constraint],
            ),
            Err(MaterialInputSpecError::DuplicateConstraint { material })
        );
    }

    #[test]
    fn lot_spec_requires_composition_to_contain_host_material() {
        let host = MaterialId::new(3);
        let other = MaterialId::new(4);
        let composition = MaterialComposition::new(vec![CompositionComponent::new(
            other,
            COMPOSITION_PARTS_PER_MILLION,
        )])
        .unwrap_or_else(|error| panic!("composition fixture failed: {error}"));

        assert_eq!(
            MaterialLotSpec::with_composition(
                CommodityKey::new(host, FormId::new(1)),
                Mass::from_milligrams(10),
                Temperature::from_millikelvin(300_000),
                composition,
            ),
            Err(MaterialLotSpecError::MissingHostMaterial { host })
        );
    }
}
