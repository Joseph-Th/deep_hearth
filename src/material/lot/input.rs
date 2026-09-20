//! Composition-aware material requirements for process inputs.

use std::error::Error;
use std::fmt::{Display, Formatter};

use super::super::{
    COMPOSITION_PARTS_PER_MILLION, CommodityKey, CompositionConstraint, MaterialComposition,
    MaterialId,
};
use crate::core::quantity::Mass;

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

    /// Builds an input requirement that accepts only pure matter matching the commodity host.
    #[must_use]
    pub fn pure(commodity: CommodityKey, mass: Mass) -> Self {
        assert!(
            !mass.is_zero(),
            "material input specification mass must be nonzero"
        );
        let purity = CompositionConstraint::new(
            commodity.material(),
            COMPOSITION_PARTS_PER_MILLION,
            COMPOSITION_PARTS_PER_MILLION,
        )
        .unwrap_or_else(|error| {
            panic!("pure material input specification has an invalid host constraint: {error}")
        });
        Self {
            commodity,
            mass,
            constraints: vec![purity],
        }
    }

    /// Builds a composition-constrained input requirement in canonical material-ID order.
    ///
    /// Runtime lots must contain their commodity host material, so the constraint set must leave at
    /// least one normalized part for that host even when the host itself has no positive minimum.
    pub fn with_constraints(
        commodity: CommodityKey,
        mass: Mass,
        mut constraints: Vec<CompositionConstraint>,
    ) -> Result<Self, MaterialInputSpecError> {
        if mass.is_zero() {
            return Err(MaterialInputSpecError::ZeroMass);
        }
        let minimum_total_ppm = canonicalize_constraints(&mut constraints)?;
        let host_constraint = constraints
            .iter()
            .find(|constraint| constraint.material() == commodity.material());
        if matches!(host_constraint, Some(constraint) if constraint.maximum_parts_per_million() == 0)
        {
            return Err(MaterialInputSpecError::HostExcluded {
                host: commodity.material(),
            });
        }
        let host_minimum_ppm = host_constraint
            .map(|constraint| constraint.minimum_parts_per_million())
            .unwrap_or(0);
        let minimum_required_ppm = minimum_total_ppm + u64::from(host_minimum_ppm == 0);
        if minimum_required_ppm > u64::from(COMPOSITION_PARTS_PER_MILLION) {
            return Err(MaterialInputSpecError::ImpossibleMinimumTotal {
                total_ppm: minimum_required_ppm,
            });
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

    /// Returns whether this requirement admits only pure matter of its commodity host material.
    #[must_use]
    pub(crate) fn requires_pure_material(&self) -> bool {
        matches!(
            self.constraints.as_slice(),
            [constraint]
                if constraint.material() == self.commodity.material()
                    && constraint.minimum_parts_per_million() == COMPOSITION_PARTS_PER_MILLION
                    && constraint.maximum_parts_per_million() == COMPOSITION_PARTS_PER_MILLION
        )
    }

    #[must_use]
    pub fn is_satisfied_by(&self, composition: &MaterialComposition) -> bool {
        self.constraints
            .iter()
            .all(|constraint| constraint.is_satisfied_by(composition))
    }
}

fn canonicalize_constraints(
    constraints: &mut [CompositionConstraint],
) -> Result<u64, MaterialInputSpecError> {
    constraints.sort_by_key(|constraint| constraint.material());
    for pair in constraints.windows(2) {
        if pair[0].material() == pair[1].material() {
            return Err(MaterialInputSpecError::DuplicateConstraint {
                material: pair[0].material(),
            });
        }
    }

    let maximum_total_ppm = u64::from(COMPOSITION_PARTS_PER_MILLION);
    let mut minimum_total_ppm = 0_u64;
    for constraint in constraints {
        minimum_total_ppm += u64::from(constraint.minimum_parts_per_million());
        if minimum_total_ppm > maximum_total_ppm {
            return Err(MaterialInputSpecError::ImpossibleMinimumTotal {
                total_ppm: minimum_total_ppm,
            });
        }
    }
    Ok(minimum_total_ppm)
}

/// Construction failure for a composition-aware material input requirement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterialInputSpecError {
    ZeroMass,
    DuplicateConstraint { material: MaterialId },
    HostExcluded { host: MaterialId },
    ImpossibleMinimumTotal { total_ppm: u64 },
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
            Self::HostExcluded { host } => write!(
                formatter,
                "material input specification excludes commodity host material {}",
                host.value()
            ),
            Self::ImpossibleMinimumTotal { total_ppm } => write!(
                formatter,
                "material input specification requires combined minimum fractions of {total_ppm} ppm including its host material, exceeding {COMPOSITION_PARTS_PER_MILLION} ppm"
            ),
        }
    }
}

impl Error for MaterialInputSpecError {}
