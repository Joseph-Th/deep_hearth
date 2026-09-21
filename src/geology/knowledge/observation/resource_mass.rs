//! Bounded actor-visible estimates of extractable geological body mass.

use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Deserializer, Serialize};

use crate::core::quantity::Mass;
use crate::material::MaterialId;

use super::{GeologicalEvidenceKind, MaterialAbundanceEstimate};

/// Conservative estimate of one fully localized geological body's remaining extractable mass.
///
/// The interval contains no geological owner identity. Physical sampling can bound the scale of a
/// local opportunity without exposing exact hidden reserve state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub struct ResourceMassEstimate {
    lower: Mass,
    upper: Mass,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ResourceMassEstimateRepresentation {
    lower: Mass,
    upper: Mass,
}

impl<'de> Deserialize<'de> for ResourceMassEstimate {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let representation = ResourceMassEstimateRepresentation::deserialize(deserializer)?;
        Self::new(representation.lower, representation.upper).map_err(serde::de::Error::custom)
    }
}

impl ResourceMassEstimate {
    pub fn new(lower: Mass, upper: Mass) -> Result<Self, ResourceMassEstimateError> {
        if upper.is_zero() {
            return Err(ResourceMassEstimateError::ZeroUpperBound);
        }
        if lower == upper {
            return Err(ResourceMassEstimateError::ZeroWidth { bound: lower });
        }
        if lower > upper {
            return Err(ResourceMassEstimateError::InvertedBounds { lower, upper });
        }
        Ok(Self { lower, upper })
    }

    #[must_use]
    pub const fn lower(self) -> Mass {
        self.lower
    }

    #[must_use]
    pub const fn upper(self) -> Mass {
        self.upper
    }

    #[must_use]
    pub fn width(self) -> Mass {
        self.upper
            .checked_sub(self.lower)
            .unwrap_or_else(|| unreachable!("validated resource-mass bounds are ordered"))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceMassEstimateError {
    ZeroUpperBound,
    ZeroWidth { bound: Mass },
    InvertedBounds { lower: Mass, upper: Mass },
}

impl Display for ResourceMassEstimateError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroUpperBound => {
                formatter.write_str("geological resource-mass upper bound must be nonzero")
            }
            Self::ZeroWidth { bound } => write!(
                formatter,
                "geological resource-mass estimate at {} mg must retain nonzero uncertainty width",
                bound.milligrams()
            ),
            Self::InvertedBounds { lower, upper } => write!(
                formatter,
                "geological resource-mass lower bound {} mg exceeds upper bound {} mg",
                lower.milligrams(),
                upper.milligrams()
            ),
        }
    }
}

impl Error for ResourceMassEstimateError {}

#[cfg(test)]
#[path = "resource_mass_tests.rs"]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::geology) enum ResourceMassContextError {
    UnsupportedEvidence { evidence: GeologicalEvidenceKind },
    AmbiguousFindings { count: usize },
    PresenceNotDefinite { material: MaterialId },
}

pub(in crate::geology) fn validate_resource_mass_context(
    evidence: GeologicalEvidenceKind,
    findings: &[MaterialAbundanceEstimate],
    resource_mass: Option<ResourceMassEstimate>,
) -> Result<(), ResourceMassContextError> {
    if resource_mass.is_none() {
        return Ok(());
    }
    if !evidence.supports_resource_mass() {
        return Err(ResourceMassContextError::UnsupportedEvidence { evidence });
    }
    let [finding] = findings else {
        return Err(ResourceMassContextError::AmbiguousFindings {
            count: findings.len(),
        });
    };
    if finding.lower_ppm() == 0 {
        return Err(ResourceMassContextError::PresenceNotDefinite {
            material: finding.material(),
        });
    }
    Ok(())
}
