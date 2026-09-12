//! Persisted geological evidence identities and bounded actor-visible measurements.

use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Deserializer, Serialize};

use crate::core::quantity::Pressure;
use crate::core::time::SimulationTick;
use crate::material::MaterialId;
use crate::spatial::VoxelBounds;

pub(in crate::geology) const PARTS_PER_MILLION: u32 =
    crate::core::arithmetic::NORMALIZED_PARTS_PER_MILLION;

/// Persistent identity of one acquired geological observation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GeologicalObservationId(u32);

impl GeologicalObservationId {
    #[must_use]
    pub const fn new(value: u32) -> Self {
        assert!(value != 0, "geological observation id must be nonzero");
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }
}

/// Physical or observational provenance for geological evidence.
///
/// These are evidence sources, not technology levels. Information quality is represented by the
/// quantitative spatial footprint and abundance bounds recorded by the resolving instrument or
/// sampling system.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum GeologicalEvidenceKind {
    SurfaceExposure,
    LooseIndicator,
    PannedConcentrate,
    ExcavationSample,
    CoreSample,
    LaboratoryAssay,
    MagneticSurvey,
    ElectricalSurvey,
    SeismicSurvey,
}

impl GeologicalEvidenceKind {
    /// Whether this evidence provenance can directly measure host-rock excavation resistance.
    pub(crate) const fn supports_excavation_hardness(self) -> bool {
        matches!(self, Self::ExcavationSample | Self::CoreSample)
    }
}

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

/// Persisted geological observation acquired at one simulation tick.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeologicalObservationRecord {
    pub(in crate::geology) id: GeologicalObservationId,
    pub(in crate::geology) region: VoxelBounds,
    pub(in crate::geology) evidence: GeologicalEvidenceKind,
    pub(in crate::geology) findings: Vec<MaterialAbundanceEstimate>,
    pub(in crate::geology) excavation_hardness: Option<ExcavationHardnessEstimate>,
    pub(in crate::geology) observed_at: SimulationTick,
}

impl GeologicalObservationRecord {
    #[must_use]
    pub const fn id(&self) -> GeologicalObservationId {
        self.id
    }

    #[must_use]
    pub const fn region(&self) -> VoxelBounds {
        self.region
    }

    #[must_use]
    pub const fn evidence(&self) -> GeologicalEvidenceKind {
        self.evidence
    }

    #[must_use]
    pub fn findings(&self) -> &[MaterialAbundanceEstimate] {
        &self.findings
    }

    /// Returns the acquired excavation-resistance band when this observation physically measured it.
    #[must_use]
    pub const fn excavation_hardness(&self) -> Option<ExcavationHardnessEstimate> {
        self.excavation_hardness
    }

    #[must_use]
    pub const fn observed_at(&self) -> SimulationTick {
        self.observed_at
    }

    #[must_use]
    pub fn finding(&self, material: MaterialId) -> Option<MaterialAbundanceEstimate> {
        self.findings
            .binary_search_by_key(&material, |finding| finding.material())
            .ok()
            .map(|index| self.findings[index])
    }
}
