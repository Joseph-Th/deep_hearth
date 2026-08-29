//! Canonical particulate size ranges and weighted distributions for material lots.

use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Deserializer, Serialize};

use crate::core::arithmetic::greatest_common_divisor_u32;
use crate::core::quantity::Length;

/// Compact authoritative diameter envelope for particulate material.
///
/// This intentionally records only guaranteed bounds, not a size distribution. Systems such as
/// screening must not infer a mass fraction from this range when the bounds straddle a screen cut.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub struct ParticleSizeRange {
    minimum_diameter: Length,
    maximum_diameter: Length,
}

impl ParticleSizeRange {
    pub fn new(
        minimum_diameter: Length,
        maximum_diameter: Length,
    ) -> Result<Self, ParticleSizeRangeError> {
        if minimum_diameter.is_zero() {
            return Err(ParticleSizeRangeError::ZeroMinimumDiameter);
        }
        if maximum_diameter.is_zero() {
            return Err(ParticleSizeRangeError::ZeroMaximumDiameter);
        }
        if minimum_diameter > maximum_diameter {
            return Err(ParticleSizeRangeError::MinimumExceedsMaximum {
                minimum: minimum_diameter,
                maximum: maximum_diameter,
            });
        }
        Ok(Self {
            minimum_diameter,
            maximum_diameter,
        })
    }

    #[must_use]
    pub const fn minimum_diameter(self) -> Length {
        self.minimum_diameter
    }

    #[must_use]
    pub const fn maximum_diameter(self) -> Length {
        self.maximum_diameter
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ParticleSizeRangeRepresentation {
    minimum_diameter: Length,
    maximum_diameter: Length,
}

impl<'de> Deserialize<'de> for ParticleSizeRange {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let representation = ParticleSizeRangeRepresentation::deserialize(deserializer)?;
        Self::new(
            representation.minimum_diameter,
            representation.maximum_diameter,
        )
        .map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParticleSizeRangeError {
    ZeroMinimumDiameter,
    ZeroMaximumDiameter,
    MinimumExceedsMaximum { minimum: Length, maximum: Length },
}

impl Display for ParticleSizeRangeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroMinimumDiameter => {
                formatter.write_str("particle-size minimum diameter must be nonzero")
            }
            Self::ZeroMaximumDiameter => {
                formatter.write_str("particle-size maximum diameter must be nonzero")
            }
            Self::MinimumExceedsMaximum { minimum, maximum } => write!(
                formatter,
                "particle-size minimum {} um exceeds maximum {} um",
                minimum.micrometers(),
                maximum.micrometers()
            ),
        }
    }
}

impl Error for ParticleSizeRangeError {}

/// One explicitly resolved particulate size class and its relative mass weight.
///
/// A class represents material known to lie somewhere inside its diameter bounds. The weight is a
/// relative mass weight; `ParticleSizeDistribution` reduces all class weights by their greatest
/// common divisor so physically equivalent ratios have one canonical persistent representation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ParticleSizeClass {
    range: ParticleSizeRange,
    weight: u32,
}

impl ParticleSizeClass {
    pub fn new(range: ParticleSizeRange, weight: u32) -> Result<Self, ParticleSizeClassError> {
        if weight == 0 {
            return Err(ParticleSizeClassError::ZeroWeight);
        }
        Ok(Self { range, weight })
    }

    #[must_use]
    pub const fn range(self) -> ParticleSizeRange {
        self.range
    }

    #[must_use]
    pub const fn weight(self) -> u32 {
        self.weight
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParticleSizeClassError {
    ZeroWeight,
}

impl Display for ParticleSizeClassError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroWeight => formatter.write_str("particle-size class weight must be nonzero"),
        }
    }
}

impl Error for ParticleSizeClassError {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ParticleSizeClassRepresentation {
    range: ParticleSizeRange,
    weight: u32,
}

impl<'de> Deserialize<'de> for ParticleSizeClass {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let representation = ParticleSizeClassRepresentation::deserialize(deserializer)?;
        Self::new(representation.range, representation.weight).map_err(serde::de::Error::custom)
    }
}

/// Canonical weighted particle-size classes for one homogeneous particulate material lot.
///
/// Classes are sorted by diameter and may contain gaps, which mean that no represented mass is
/// known to occupy that interval. Classes may not overlap or touch because an exact boundary value
/// would otherwise have ambiguous ownership. A single class is the conservative representation of
/// an unresolved size envelope: it records bounds without pretending to know how mass is distributed
/// inside them.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub struct ParticleSizeDistribution {
    classes: Vec<ParticleSizeClass>,
}

impl ParticleSizeDistribution {
    pub fn new(mut classes: Vec<ParticleSizeClass>) -> Result<Self, ParticleSizeDistributionError> {
        classes.sort_by_key(|class| {
            (
                class.range().minimum_diameter(),
                class.range().maximum_diameter(),
            )
        });
        let divisor = classes.iter().fold(0_u32, |divisor, class| {
            greatest_common_divisor_u32(divisor, class.weight())
        });
        if divisor > 1 {
            for class in &mut classes {
                class.weight /= divisor;
            }
        }
        let distribution = Self { classes };
        distribution.validate()?;
        Ok(distribution)
    }

    pub fn validate(&self) -> Result<(), ParticleSizeDistributionError> {
        if self.classes.is_empty() {
            return Err(ParticleSizeDistributionError::Empty);
        }
        let mut total_weight = 0_u64;
        let mut common_divisor = 0_u32;
        let mut previous: Option<ParticleSizeRange> = None;
        for class in &self.classes {
            if class.weight() == 0 {
                return Err(ParticleSizeDistributionError::ZeroWeight {
                    range: class.range(),
                });
            }
            if let Some(previous_range) = previous
                && previous_range.maximum_diameter() >= class.range().minimum_diameter()
            {
                return Err(ParticleSizeDistributionError::OverlappingClasses {
                    previous: previous_range,
                    current: class.range(),
                });
            }
            total_weight = total_weight
                .checked_add(u64::from(class.weight()))
                .ok_or(ParticleSizeDistributionError::WeightSumOverflow)?;
            common_divisor = greatest_common_divisor_u32(common_divisor, class.weight());
            previous = Some(class.range());
        }
        if common_divisor > 1 {
            return Err(ParticleSizeDistributionError::NonCanonicalWeights { common_divisor });
        }
        Ok(())
    }

    #[must_use]
    pub fn classes(&self) -> &[ParticleSizeClass] {
        &self.classes
    }

    #[must_use]
    pub fn envelope(&self) -> ParticleSizeRange {
        let first = match self.classes.first() {
            Some(first) => first.range(),
            None => panic!("validated particle-size distribution must contain a class"),
        };
        let last = match self.classes.last() {
            Some(last) => last.range(),
            None => panic!("validated particle-size distribution must contain a class"),
        };
        ParticleSizeRange {
            minimum_diameter: first.minimum_diameter(),
            maximum_diameter: last.maximum_diameter(),
        }
    }

    #[must_use]
    pub fn total_weight(&self) -> u64 {
        self.classes
            .iter()
            .map(|class| u64::from(class.weight()))
            .sum()
    }
}

impl From<ParticleSizeRange> for ParticleSizeDistribution {
    fn from(range: ParticleSizeRange) -> Self {
        Self {
            classes: vec![ParticleSizeClass { range, weight: 1 }],
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ParticleSizeDistributionRepresentation {
    classes: Vec<ParticleSizeClass>,
}

impl<'de> Deserialize<'de> for ParticleSizeDistribution {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let representation = ParticleSizeDistributionRepresentation::deserialize(deserializer)?;
        let distribution = Self {
            classes: representation.classes,
        };
        distribution.validate().map_err(serde::de::Error::custom)?;
        Ok(distribution)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParticleSizeDistributionError {
    Empty,
    ZeroWeight {
        range: ParticleSizeRange,
    },
    OverlappingClasses {
        previous: ParticleSizeRange,
        current: ParticleSizeRange,
    },
    NonCanonicalWeights {
        common_divisor: u32,
    },
    WeightSumOverflow,
}

impl Display for ParticleSizeDistributionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => formatter.write_str("particle-size distribution must contain a class"),
            Self::ZeroWeight { range } => write!(
                formatter,
                "particle-size class {}..={} um has zero weight",
                range.minimum_diameter().micrometers(),
                range.maximum_diameter().micrometers()
            ),
            Self::OverlappingClasses { previous, current } => write!(
                formatter,
                "particle-size classes {}..={} um and {}..={} um overlap or share a boundary",
                previous.minimum_diameter().micrometers(),
                previous.maximum_diameter().micrometers(),
                current.minimum_diameter().micrometers(),
                current.maximum_diameter().micrometers()
            ),
            Self::NonCanonicalWeights { common_divisor } => write!(
                formatter,
                "particle-size class weights are not reduced to their canonical ratio; common divisor is {common_divisor}"
            ),
            Self::WeightSumOverflow => {
                formatter.write_str("particle-size distribution weight sum overflowed")
            }
        }
    }
}

impl Error for ParticleSizeDistributionError {}

#[cfg(test)]
#[path = "particle_tests.rs"]
mod tests;
