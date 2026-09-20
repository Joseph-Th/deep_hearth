//! Bounded actor-visible geological material-abundance estimates.

use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Deserializer, Serialize};

use crate::material::MaterialId;

pub(in crate::geology) const PARTS_PER_MILLION: u32 =
    crate::core::arithmetic::NORMALIZED_PARTS_PER_MILLION;

/// Bounded estimate of one material's local mass fraction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub struct MaterialAbundanceEstimate {
    material: MaterialId,
    lower_ppm: u32,
    upper_ppm: u32,
}

impl MaterialAbundanceEstimate {
    pub fn new(
        material: MaterialId,
        lower_ppm: u32,
        upper_ppm: u32,
    ) -> Result<Self, MaterialAbundanceEstimateError> {
        if lower_ppm > PARTS_PER_MILLION {
            return Err(MaterialAbundanceEstimateError::AboveUnity {
                bound: AbundanceBound::Lower,
                value: lower_ppm,
            });
        }
        if upper_ppm > PARTS_PER_MILLION {
            return Err(MaterialAbundanceEstimateError::AboveUnity {
                bound: AbundanceBound::Upper,
                value: upper_ppm,
            });
        }
        if lower_ppm > upper_ppm {
            return Err(MaterialAbundanceEstimateError::InvertedBounds {
                lower_ppm,
                upper_ppm,
            });
        }
        Ok(Self {
            material,
            lower_ppm,
            upper_ppm,
        })
    }

    #[must_use]
    pub const fn material(self) -> MaterialId {
        self.material
    }

    #[must_use]
    pub const fn lower_ppm(self) -> u32 {
        self.lower_ppm
    }

    #[must_use]
    pub const fn upper_ppm(self) -> u32 {
        self.upper_ppm
    }

    #[must_use]
    pub const fn width_ppm(self) -> u32 {
        self.upper_ppm - self.lower_ppm
    }
}

pub(in crate::geology) fn total_lower_bound_ppm(findings: &[MaterialAbundanceEstimate]) -> u64 {
    findings
        .iter()
        .map(|finding| u64::from(finding.lower_ppm()))
        .sum()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MaterialAbundanceEstimateRepresentation {
    material: MaterialId,
    lower_ppm: u32,
    upper_ppm: u32,
}

impl<'de> Deserialize<'de> for MaterialAbundanceEstimate {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let representation = MaterialAbundanceEstimateRepresentation::deserialize(deserializer)?;
        Self::new(
            representation.material,
            representation.lower_ppm,
            representation.upper_ppm,
        )
        .map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AbundanceBound {
    Lower,
    Upper,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterialAbundanceEstimateError {
    AboveUnity { bound: AbundanceBound, value: u32 },
    InvertedBounds { lower_ppm: u32, upper_ppm: u32 },
}

impl Display for MaterialAbundanceEstimateError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AboveUnity { bound, value } => write!(
                formatter,
                "geological abundance {bound:?} bound {value} ppm exceeds 1000000 ppm"
            ),
            Self::InvertedBounds {
                lower_ppm,
                upper_ppm,
            } => write!(
                formatter,
                "geological abundance lower bound {lower_ppm} ppm exceeds upper bound {upper_ppm} ppm"
            ),
        }
    }
}

impl Error for MaterialAbundanceEstimateError {}
