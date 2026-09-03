//! Owns persistent geological observations and conservative read-only knowledge assessment.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Deserializer, Serialize};

use crate::core::quantity::Pressure;
use crate::core::time::SimulationTick;
use crate::material::MaterialId;
use crate::spatial::VoxelBounds;

pub(super) const PARTS_PER_MILLION: u32 = 1_000_000;

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

pub(super) fn total_lower_bound_ppm(findings: &[MaterialAbundanceEstimate]) -> u64 {
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExcavationHardnessEstimate {
    lower: Pressure,
    upper: Pressure,
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

/// Persisted geological observation acquired at one simulation tick.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeologicalObservationRecord {
    pub(super) id: GeologicalObservationId,
    pub(super) region: VoxelBounds,
    pub(super) evidence: GeologicalEvidenceKind,
    pub(super) findings: Vec<MaterialAbundanceEstimate>,
    pub(super) excavation_hardness: Option<ExcavationHardnessEstimate>,
    pub(super) observed_at: SimulationTick,
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

/// Player-accessible geological knowledge, separate from exact authoritative deposit truth.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeologicalKnowledgeState {
    revision: u64,
    next_observation_id: u32,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_btree_map_no_duplicates")]
    observations: BTreeMap<GeologicalObservationId, GeologicalObservationRecord>,
    #[serde(skip)]
    observations_by_material: BTreeMap<MaterialId, BTreeSet<GeologicalObservationId>>,
}

impl GeologicalKnowledgeState {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            revision: 0,
            next_observation_id: 1,
            observations: BTreeMap::new(),
            observations_by_material: BTreeMap::new(),
        }
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub(super) const fn next_observation_id(&self) -> u32 {
        self.next_observation_id
    }

    #[must_use]
    pub fn get_observation(
        &self,
        id: GeologicalObservationId,
    ) -> Option<&GeologicalObservationRecord> {
        self.observations.get(&id)
    }

    pub fn observations(&self) -> impl Iterator<Item = &GeologicalObservationRecord> {
        self.observations.values()
    }

    pub fn observation_ids_for_material(
        &self,
        material: MaterialId,
    ) -> impl Iterator<Item = GeologicalObservationId> + '_ {
        self.observations_by_material
            .get(&material)
            .into_iter()
            .flat_map(|ids| ids.iter().copied())
    }

    /// Iterates materials for which at least one observation has been acquired.
    pub fn known_materials(&self) -> impl Iterator<Item = MaterialId> + '_ {
        self.observations_by_material.keys().copied()
    }

    pub(crate) fn rebuild_derived_indexes(&mut self) {
        let mut observations_by_material =
            BTreeMap::<MaterialId, BTreeSet<GeologicalObservationId>>::new();
        for observation in self.observations.values() {
            for finding in &observation.findings {
                observations_by_material
                    .entry(finding.material())
                    .or_default()
                    .insert(observation.id);
            }
        }
        self.observations_by_material = observations_by_material;
    }

    pub(super) fn insert_observation(
        &mut self,
        record: GeologicalObservationRecord,
        next_observation_id: u32,
        next_revision: u64,
    ) {
        let id = record.id;
        assert_eq!(
            id.value(),
            self.next_observation_id,
            "geological observation allocation must consume the current identity cursor"
        );
        assert_eq!(
            self.next_observation_id.checked_add(1),
            Some(next_observation_id),
            "geological observation allocation must advance the identity cursor exactly once"
        );
        assert_eq!(
            self.revision.checked_add(1),
            Some(next_revision),
            "geological observation allocation must advance the owner revision exactly once"
        );
        assert!(
            !self.observations.contains_key(&id),
            "validated geological observation ID must be unique"
        );
        for finding in &record.findings {
            self.observations_by_material
                .entry(finding.material())
                .or_default()
                .insert(id);
        }
        let replaced = self.observations.insert(id, record);
        assert!(replaced.is_none(), "observation uniqueness was prechecked");
        self.next_observation_id = next_observation_id;
        self.revision = next_revision;
    }

    pub(crate) fn has_valid_id_cursor(&self) -> bool {
        self.next_observation_id != 0
            && self
                .observations
                .keys()
                .next_back()
                .is_none_or(|highest| highest.value() < self.next_observation_id)
    }
}

mod assessment;
mod validation;

pub use assessment::{
    GeologicalEvidenceConsistency, GeologicalKnowledgeAssessment, GeologicalKnowledgeMap,
    assess_geological_knowledge, build_geological_knowledge_map,
};
pub use validation::GeologicalKnowledgeValidationError;
pub(crate) use validation::validate_loaded_geological_knowledge;

#[cfg(test)]
#[path = "knowledge_tests.rs"]
mod tests;
