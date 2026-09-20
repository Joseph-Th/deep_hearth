//! Bounded actor-visible excavation-hardness estimates and evidence-context validation.

use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Deserializer, Serialize};

use crate::core::quantity::Pressure;
use crate::material::MaterialId;

use super::{GeologicalEvidenceKind, MaterialAbundanceEstimate};

/// Bounded actor-visible estimate of the excavation resistance in one observed region.
///
/// The interval deliberately contains no geological owner identity. Physical sampling may narrow
/// this band enough to choose suitable extraction tooling without exposing exact hidden deposit
/// state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub struct ExcavationHardnessEstimate {
    pub(super) lower: Pressure,
    pub(super) upper: Pressure,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExcavationHardnessEstimateRepresentation {
    lower: Pressure,
    upper: Pressure,
}

impl<'de> Deserialize<'de> for ExcavationHardnessEstimate {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let representation = ExcavationHardnessEstimateRepresentation::deserialize(deserializer)?;
        Self::new(representation.lower, representation.upper).map_err(serde::de::Error::custom)
    }
}

impl ExcavationHardnessEstimate {
    pub fn new(lower: Pressure, upper: Pressure) -> Result<Self, ExcavationHardnessEstimateError> {
        if upper.is_zero() {
            return Err(ExcavationHardnessEstimateError::ZeroUpperBound);
        }
        if lower > upper {
            return Err(ExcavationHardnessEstimateError::InvertedBounds { lower, upper });
        }
        Ok(Self { lower, upper })
    }

    #[must_use]
    pub const fn lower(self) -> Pressure {
        self.lower
    }

    #[must_use]
    pub const fn upper(self) -> Pressure {
        self.upper
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExcavationHardnessEstimateError {
    ZeroUpperBound,
    InvertedBounds { lower: Pressure, upper: Pressure },
}

impl Display for ExcavationHardnessEstimateError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroUpperBound => {
                formatter.write_str("geological excavation-hardness upper bound must be nonzero")
            }
            Self::InvertedBounds { lower, upper } => write!(
                formatter,
                "geological excavation-hardness lower bound {} Pa exceeds upper bound {} Pa",
                lower.pascals(),
                upper.pascals()
            ),
        }
    }
}

impl Error for ExcavationHardnessEstimateError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::geology) enum ExcavationHardnessContextError {
    UnsupportedEvidence { evidence: GeologicalEvidenceKind },
    AmbiguousFindings { count: usize },
    PresenceNotDefinite { material: MaterialId },
}

/// Validates the information context required for an excavation-resistance observation.
///
/// One hardness band is meaningful only for one definitely present material acquired by a physical
/// sample. Runtime admission and trusted-load replay share this rule so hardness metadata cannot
/// become a hidden-presence side channel.
pub(in crate::geology) fn validate_excavation_hardness_context(
    evidence: GeologicalEvidenceKind,
    findings: &[MaterialAbundanceEstimate],
    excavation_hardness: Option<ExcavationHardnessEstimate>,
) -> Result<(), ExcavationHardnessContextError> {
    if excavation_hardness.is_none() {
        return Ok(());
    }
    if !evidence.supports_excavation_hardness() {
        return Err(ExcavationHardnessContextError::UnsupportedEvidence { evidence });
    }
    let [finding] = findings else {
        return Err(ExcavationHardnessContextError::AmbiguousFindings {
            count: findings.len(),
        });
    };
    if finding.lower_ppm() == 0 {
        return Err(ExcavationHardnessContextError::PresenceNotDefinite {
            material: finding.material(),
        });
    }
    Ok(())
}
