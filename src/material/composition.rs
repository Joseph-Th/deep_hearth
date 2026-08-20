//! Canonical material composition values and composition constraints.

use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Deserializer, Serialize};

use super::{COMPOSITION_PARTS_PER_MILLION, MaterialId};
use crate::core::quantity::Mass;

/// One constituent fraction in a normalized runtime material composition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompositionComponent {
    material: MaterialId,
    parts_per_million: u32,
}

impl CompositionComponent {
    #[must_use]
    pub const fn new(material: MaterialId, parts_per_million: u32) -> Self {
        Self {
            material,
            parts_per_million,
        }
    }

    #[must_use]
    pub const fn material(self) -> MaterialId {
        self.material
    }

    #[must_use]
    pub const fn parts_per_million(self) -> u32 {
        self.parts_per_million
    }
}

/// Structural validation failure for a normalized material composition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompositionError {
    Empty,
    ZeroMaterialId,
    ZeroFraction {
        material: MaterialId,
    },
    DuplicateMaterial {
        material: MaterialId,
    },
    UnsortedMaterials {
        previous: MaterialId,
        current: MaterialId,
    },
    FractionSumOverflow,
    FractionSumMismatch {
        found: u64,
    },
}

impl Display for CompositionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => formatter.write_str("material composition must contain a constituent"),
            Self::ZeroMaterialId => {
                formatter.write_str("material composition must not reference material id zero")
            }
            Self::ZeroFraction { material } => write!(
                formatter,
                "material composition contains zero ppm for material {}",
                material.value()
            ),
            Self::DuplicateMaterial { material } => write!(
                formatter,
                "material composition contains duplicate material {}",
                material.value()
            ),
            Self::UnsortedMaterials { previous, current } => write!(
                formatter,
                "material composition is not sorted: material {} precedes {}",
                previous.value(),
                current.value()
            ),
            Self::FractionSumOverflow => {
                formatter.write_str("material composition fraction sum overflowed")
            }
            Self::FractionSumMismatch { found } => write!(
                formatter,
                "material composition totals {found} ppm instead of {COMPOSITION_PARTS_PER_MILLION} ppm"
            ),
        }
    }
}

impl Error for CompositionError {}

/// Canonical normalized composition for one homogeneous material lot.
///
/// Components are mass fractions sorted by stable material ID and sum to exactly one million parts
/// per million. This preserves deterministic serialization and bounded integer chemistry without
/// multiplying authored material definitions for every alloy ratio or ore grade.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct MaterialComposition {
    components: Vec<CompositionComponent>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MaterialCompositionRepresentation {
    components: Vec<CompositionComponent>,
}

impl<'de> Deserialize<'de> for MaterialComposition {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let representation = MaterialCompositionRepresentation::deserialize(deserializer)?;
        let composition = Self {
            components: representation.components,
        };
        composition.validate().map_err(serde::de::Error::custom)?;
        Ok(composition)
    }
}

impl MaterialComposition {
    /// Builds a normalized composition, sorting components into canonical material-ID order.
    pub fn new(mut components: Vec<CompositionComponent>) -> Result<Self, CompositionError> {
        components.sort_by_key(|component| component.material());
        let composition = Self { components };
        composition.validate()?;
        Ok(composition)
    }

    /// Builds a pure single-material composition.
    #[must_use]
    pub fn pure(material: MaterialId) -> Self {
        assert!(
            material.value() != 0,
            "material composition host id must be nonzero"
        );
        Self {
            components: vec![CompositionComponent::new(
                material,
                COMPOSITION_PARTS_PER_MILLION,
            )],
        }
    }

    /// Validates canonical ordering and exact normalization, including after deserialization.
    pub fn validate(&self) -> Result<(), CompositionError> {
        if self.components.is_empty() {
            return Err(CompositionError::Empty);
        }

        let mut total = 0_u64;
        let mut previous = None;
        for component in &self.components {
            if component.material().value() == 0 {
                return Err(CompositionError::ZeroMaterialId);
            }
            if component.parts_per_million() == 0 {
                return Err(CompositionError::ZeroFraction {
                    material: component.material(),
                });
            }
            if let Some(previous_material) = previous {
                if component.material() == previous_material {
                    return Err(CompositionError::DuplicateMaterial {
                        material: component.material(),
                    });
                }
                if component.material() < previous_material {
                    return Err(CompositionError::UnsortedMaterials {
                        previous: previous_material,
                        current: component.material(),
                    });
                }
            }
            total = total
                .checked_add(u64::from(component.parts_per_million()))
                .ok_or(CompositionError::FractionSumOverflow)?;
            previous = Some(component.material());
        }
        if total != u64::from(COMPOSITION_PARTS_PER_MILLION) {
            return Err(CompositionError::FractionSumMismatch { found: total });
        }
        Ok(())
    }

    /// Returns canonical constituent entries in stable material-ID order.
    #[must_use]
    pub fn components(&self) -> &[CompositionComponent] {
        &self.components
    }

    /// Returns the sole material when this composition is exactly pure.
    #[must_use]
    pub fn pure_material(&self) -> Option<MaterialId> {
        match self.components.as_slice() {
            [component] if component.parts_per_million() == COMPOSITION_PARTS_PER_MILLION => {
                Some(component.material())
            }
            _ => None,
        }
    }

    /// Returns one constituent fraction, or zero when the material is absent.
    #[must_use]
    pub fn parts_per_million(&self, material: MaterialId) -> u32 {
        match self
            .components
            .binary_search_by_key(&material, |component| component.material())
        {
            Ok(index) => self.components[index].parts_per_million(),
            Err(_) => 0,
        }
    }

    /// Projects one constituent's mass by flooring at the authoritative milligram boundary.
    ///
    /// Flooring is deliberate: this query never manufactures mass through rounding. Systems that
    /// need sub-milligram conservation must persist their own process remainder explicitly.
    #[must_use]
    pub fn constituent_mass_floor(&self, total_mass: Mass, material: MaterialId) -> Mass {
        let numerator =
            u128::from(total_mass.milligrams()) * u128::from(self.parts_per_million(material));
        let milligrams = numerator / u128::from(COMPOSITION_PARTS_PER_MILLION);
        debug_assert!(milligrams <= u128::from(u64::MAX));
        Mass::from_milligrams(milligrams as u64)
    }
}

/// Inclusive constituent range required by a material-consuming operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CompositionConstraint {
    material: MaterialId,
    minimum_parts_per_million: u32,
    maximum_parts_per_million: u32,
}

impl CompositionConstraint {
    /// Builds one inclusive constituent range.
    pub fn new(
        material: MaterialId,
        minimum_parts_per_million: u32,
        maximum_parts_per_million: u32,
    ) -> Result<Self, CompositionConstraintError> {
        if material.value() == 0 {
            return Err(CompositionConstraintError::ZeroMaterialId);
        }
        if minimum_parts_per_million > maximum_parts_per_million {
            return Err(CompositionConstraintError::MinimumExceedsMaximum {
                minimum: minimum_parts_per_million,
                maximum: maximum_parts_per_million,
            });
        }
        if maximum_parts_per_million > COMPOSITION_PARTS_PER_MILLION {
            return Err(CompositionConstraintError::MaximumExceedsNormalization {
                maximum: maximum_parts_per_million,
            });
        }
        Ok(Self {
            material,
            minimum_parts_per_million,
            maximum_parts_per_million,
        })
    }

    #[must_use]
    pub const fn material(self) -> MaterialId {
        self.material
    }

    #[must_use]
    pub const fn minimum_parts_per_million(self) -> u32 {
        self.minimum_parts_per_million
    }

    #[must_use]
    pub const fn maximum_parts_per_million(self) -> u32 {
        self.maximum_parts_per_million
    }

    #[must_use]
    pub fn is_satisfied_by(self, composition: &MaterialComposition) -> bool {
        let fraction = composition.parts_per_million(self.material);
        fraction >= self.minimum_parts_per_million && fraction <= self.maximum_parts_per_million
    }
}

/// Invalid constituent range for a material input specification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompositionConstraintError {
    ZeroMaterialId,
    MinimumExceedsMaximum { minimum: u32, maximum: u32 },
    MaximumExceedsNormalization { maximum: u32 },
}

impl Display for CompositionConstraintError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroMaterialId => {
                formatter.write_str("composition constraint material id must be nonzero")
            }
            Self::MinimumExceedsMaximum { minimum, maximum } => write!(
                formatter,
                "composition constraint minimum {minimum} ppm exceeds maximum {maximum} ppm"
            ),
            Self::MaximumExceedsNormalization { maximum } => write!(
                formatter,
                "composition constraint maximum {maximum} ppm exceeds {COMPOSITION_PARTS_PER_MILLION} ppm"
            ),
        }
    }
}

impl Error for CompositionConstraintError {}

#[cfg(all(
    test,
    any(not(feature = "test-unit-sharded"), feature = "test-unit-foundation")
))]
mod tests {
    use super::*;

    #[test]
    fn normalizes_order_and_projects_constituent_mass_without_rounding_up() {
        let copper = MaterialId::new(3);
        let slag = MaterialId::new(4);
        let composition = MaterialComposition::new(vec![
            CompositionComponent::new(slag, 275_000),
            CompositionComponent::new(copper, 725_000),
        ])
        .unwrap_or_else(|error| panic!("composition unexpectedly failed: {error}"));

        assert_eq!(composition.components()[0].material(), copper);
        assert_eq!(composition.components()[1].material(), slag);
        assert_eq!(composition.parts_per_million(copper), 725_000);
        assert_eq!(
            composition.constituent_mass_floor(Mass::from_milligrams(3), copper),
            Mass::from_milligrams(2)
        );
        assert_eq!(
            composition.constituent_mass_floor(Mass::from_milligrams(3), slag),
            Mass::ZERO
        );
    }

    #[test]
    fn rejects_non_normalized_fraction_total() {
        let result = MaterialComposition::new(vec![
            CompositionComponent::new(MaterialId::new(3), 500_000),
            CompositionComponent::new(MaterialId::new(4), 499_999),
        ]);

        assert_eq!(
            result,
            Err(CompositionError::FractionSumMismatch { found: 999_999 })
        );
    }

    #[test]
    fn deserialization_rejects_noncanonical_order() {
        let encoded = br#"{
            "components": [
                {"material":4,"parts_per_million":500000},
                {"material":3,"parts_per_million":500000}
            ]
        }"#;
        let result: Result<MaterialComposition, _> = serde_json::from_slice(encoded);

        assert!(result.is_err());
    }

    #[test]
    fn constraints_match_composition_ranges_inclusively() {
        let copper = MaterialId::new(3);
        let slag = MaterialId::new(4);
        let composition = MaterialComposition::new(vec![
            CompositionComponent::new(copper, 800_000),
            CompositionComponent::new(slag, 200_000),
        ])
        .unwrap_or_else(|error| panic!("composition unexpectedly failed: {error}"));
        let accepts = CompositionConstraint::new(copper, 800_000, 900_000)
            .unwrap_or_else(|error| panic!("constraint unexpectedly failed: {error}"));
        let rejects = CompositionConstraint::new(slag, 0, 199_999)
            .unwrap_or_else(|error| panic!("constraint unexpectedly failed: {error}"));

        assert!(accepts.is_satisfied_by(&composition));
        assert!(!rejects.is_satisfied_by(&composition));
    }
}
