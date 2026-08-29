//! Owns persistent geological observations and conservative read-only knowledge assessment.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Deserializer, Serialize};

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

/// Persisted geological observation acquired at one simulation tick.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeologicalObservationRecord {
    pub(super) id: GeologicalObservationId,
    pub(super) region: VoxelBounds,
    pub(super) evidence: GeologicalEvidenceKind,
    pub(super) findings: Vec<MaterialAbundanceEstimate>,
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

    pub(crate) const fn has_valid_id_cursor(&self) -> bool {
        self.next_observation_id != 0
    }
}

/// Deterministic regional projection suitable for geological-map presentation and planning.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeologicalKnowledgeMap {
    region: VoxelBounds,
    assessments: Vec<GeologicalKnowledgeAssessment>,
}

impl GeologicalKnowledgeMap {
    #[must_use]
    pub const fn region(&self) -> VoxelBounds {
        self.region
    }

    #[must_use]
    pub fn assessments(&self) -> &[GeologicalKnowledgeAssessment] {
        &self.assessments
    }
}

/// Compatibility of all currently relevant bounded measurements.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeologicalEvidenceConsistency {
    NoEvidence,
    /// Relevant evidence exists, but its clipped spatial footprints share no common voxel and its
    /// abundance bounds therefore cannot be compared as measurements of one locality.
    SpatiallyIncomparable,
    Compatible {
        lower_ppm: u32,
        upper_ppm: u32,
    },
    Conflicting {
        highest_lower_ppm: u32,
        lowest_upper_ppm: u32,
    },
}

/// Conservative read projection for one material within a requested region.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeologicalKnowledgeAssessment {
    material: MaterialId,
    region: VoxelBounds,
    observations: Vec<GeologicalObservationId>,
    consistency: GeologicalEvidenceConsistency,
    envelope: Option<(u32, u32)>,
    common_evidence_region: Option<VoxelBounds>,
    common_acquired_region: Option<VoxelBounds>,
    most_precise: Option<GeologicalObservationId>,
    latest_observed_at: Option<SimulationTick>,
}

impl GeologicalKnowledgeAssessment {
    #[must_use]
    pub const fn material(&self) -> MaterialId {
        self.material
    }

    #[must_use]
    pub const fn region(&self) -> VoxelBounds {
        self.region
    }

    #[must_use]
    pub fn observations(&self) -> &[GeologicalObservationId] {
        &self.observations
    }

    #[must_use]
    pub const fn consistency(&self) -> GeologicalEvidenceConsistency {
        self.consistency
    }

    #[must_use]
    pub const fn envelope(&self) -> Option<(u32, u32)> {
        self.envelope
    }

    /// Region within the query covered by every relevant observation.
    ///
    /// A compatible intersection or explicit conflict applies only where this common region exists.
    #[must_use]
    pub const fn common_evidence_region(&self) -> Option<VoxelBounds> {
        self.common_evidence_region
    }

    /// Region shared by the original acquired observation footprints before the query clips them.
    ///
    /// This prevents consumers from mistaking a narrow read query for spatial precision that was
    /// never actually acquired. `None` means the relevant observations have no common locality.
    #[must_use]
    pub(crate) const fn common_acquired_region(&self) -> Option<VoxelBounds> {
        self.common_acquired_region
    }

    #[must_use]
    pub const fn most_precise(&self) -> Option<GeologicalObservationId> {
        self.most_precise
    }

    #[must_use]
    pub const fn latest_observed_at(&self) -> Option<SimulationTick> {
        self.latest_observed_at
    }
}

type EvidencePrecisionRank = (
    Reverse<u32>,
    Reverse<u128>,
    SimulationTick,
    Reverse<GeologicalObservationId>,
);

struct GeologicalEvidenceAggregate {
    observations: Vec<GeologicalObservationId>,
    highest_lower_ppm: u32,
    lowest_upper_ppm: u32,
    envelope_lower_ppm: u32,
    envelope_upper_ppm: u32,
    common_evidence_region: Option<VoxelBounds>,
    common_acquired_region: Option<VoxelBounds>,
    most_precise: Option<EvidencePrecisionRank>,
    latest_observed_at: Option<SimulationTick>,
}

impl GeologicalEvidenceAggregate {
    fn new(region: VoxelBounds) -> Self {
        Self {
            observations: Vec::new(),
            highest_lower_ppm: 0,
            lowest_upper_ppm: PARTS_PER_MILLION,
            envelope_lower_ppm: PARTS_PER_MILLION,
            envelope_upper_ppm: 0,
            common_evidence_region: Some(region),
            common_acquired_region: None,
            most_precise: None,
            latest_observed_at: None,
        }
    }

    fn add(
        &mut self,
        id: GeologicalObservationId,
        record: &GeologicalObservationRecord,
        finding: MaterialAbundanceEstimate,
        overlap: VoxelBounds,
    ) {
        self.common_acquired_region = if self.observations.is_empty() {
            Some(record.region())
        } else {
            self.common_acquired_region
                .and_then(|common| common.intersection(record.region()))
        };
        self.observations.push(id);
        self.highest_lower_ppm = self.highest_lower_ppm.max(finding.lower_ppm());
        self.lowest_upper_ppm = self.lowest_upper_ppm.min(finding.upper_ppm());
        self.envelope_lower_ppm = self.envelope_lower_ppm.min(finding.lower_ppm());
        self.envelope_upper_ppm = self.envelope_upper_ppm.max(finding.upper_ppm());
        self.common_evidence_region = self
            .common_evidence_region
            .and_then(|common| common.intersection(overlap));
        self.latest_observed_at = Some(
            self.latest_observed_at
                .map_or(record.observed_at(), |current| {
                    current.max(record.observed_at())
                }),
        );

        let precision = (
            Reverse(finding.width_ppm()),
            Reverse(record.region().voxel_count().unwrap_or(u128::MAX)),
            record.observed_at(),
            Reverse(id),
        );
        self.most_precise = Some(
            self.most_precise
                .map_or(precision, |current| current.max(precision)),
        );
    }

    fn into_assessment(
        self,
        material: MaterialId,
        region: VoxelBounds,
    ) -> GeologicalKnowledgeAssessment {
        let has_evidence = !self.observations.is_empty();
        let common_evidence_region = has_evidence
            .then_some(self.common_evidence_region)
            .flatten();
        let common_acquired_region = has_evidence
            .then_some(self.common_acquired_region)
            .flatten();
        let consistency = if !has_evidence {
            GeologicalEvidenceConsistency::NoEvidence
        } else if common_evidence_region.is_none() {
            GeologicalEvidenceConsistency::SpatiallyIncomparable
        } else if self.highest_lower_ppm <= self.lowest_upper_ppm {
            GeologicalEvidenceConsistency::Compatible {
                lower_ppm: self.highest_lower_ppm,
                upper_ppm: self.lowest_upper_ppm,
            }
        } else {
            GeologicalEvidenceConsistency::Conflicting {
                highest_lower_ppm: self.highest_lower_ppm,
                lowest_upper_ppm: self.lowest_upper_ppm,
            }
        };
        let envelope = has_evidence.then_some((self.envelope_lower_ppm, self.envelope_upper_ppm));
        GeologicalKnowledgeAssessment {
            material,
            region,
            observations: self.observations,
            consistency,
            envelope,
            common_evidence_region,
            common_acquired_region,
            most_precise: self.most_precise.map(|rank| rank.3.0),
            latest_observed_at: self.latest_observed_at,
        }
    }
}

/// Builds a stable regional map from acquired evidence only.
///
/// Materials known elsewhere but with no evidence intersecting this region are omitted. Ordering is
/// stable by material ID because the knowledge owner's material index is a `BTreeMap`.
#[must_use]
pub fn build_geological_knowledge_map(
    state: &GeologicalKnowledgeState,
    region: VoxelBounds,
) -> GeologicalKnowledgeMap {
    let assessments = state
        .known_materials()
        .filter_map(|material| {
            let assessment = assess_geological_knowledge(state, region, material);
            (!matches!(
                assessment.consistency(),
                GeologicalEvidenceConsistency::NoEvidence
            ))
            .then_some(assessment)
        })
        .collect();
    GeologicalKnowledgeMap {
        region,
        assessments,
    }
}

/// Assesses acquired evidence without consulting hidden authoritative deposits.
///
/// Measurements are intersected as hard reported bounds only when every relevant observation shares
/// a common spatial overlap inside the query. Evidence from disjoint subregions is reported as
/// spatially incomparable instead of manufacturing either precision or contradiction. Where a
/// common locality exists, contradictory measurements are surfaced explicitly rather than averaged.
/// The broad evidence envelope remains available in every nonempty case. Precision ranking prefers
/// narrower abundance bounds, then smaller spatial footprints, then newer evidence, then lower
/// persistent identity.
#[must_use]
pub fn assess_geological_knowledge(
    state: &GeologicalKnowledgeState,
    region: VoxelBounds,
    material: MaterialId,
) -> GeologicalKnowledgeAssessment {
    let mut aggregate = GeologicalEvidenceAggregate::new(region);

    for id in state.observation_ids_for_material(material) {
        let record = match state.get_observation(id) {
            Some(record) => record,
            None => panic!(
                "runtime invariant broken: geological material index references missing observation {}",
                id.value()
            ),
        };
        let Some(overlap) = record.region().intersection(region) else {
            continue;
        };
        let finding = match record.finding(material) {
            Some(finding) => finding,
            None => panic!(
                "runtime invariant broken: geological material index references observation without material {}",
                material.value()
            ),
        };
        aggregate.add(id, record, finding, overlap);
    }
    aggregate.into_assessment(material, region)
}

mod validation;

pub use validation::GeologicalKnowledgeValidationError;
pub(crate) use validation::validate_loaded_geological_knowledge;

#[cfg(test)]
#[path = "knowledge_tests.rs"]
mod tests;
