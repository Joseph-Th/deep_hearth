//! Conservative read-only geological assessment from acquired evidence only.

use std::cmp::Reverse;

use crate::core::time::SimulationTick;
use crate::material::MaterialId;
use crate::spatial::VoxelBounds;

use super::{
    GeologicalKnowledgeState, GeologicalObservationId, GeologicalObservationRecord,
    MaterialAbundanceEstimate, PARTS_PER_MILLION,
};

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
    #[must_use]
    pub const fn common_evidence_region(&self) -> Option<VoxelBounds> {
        self.common_evidence_region
    }

    /// Region shared by the original acquired observation footprints before query clipping.
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
        GeologicalKnowledgeAssessment {
            material,
            region,
            observations: self.observations,
            consistency,
            envelope: has_evidence.then_some((self.envelope_lower_ppm, self.envelope_upper_ppm)),
            common_evidence_region,
            common_acquired_region,
            most_precise: self.most_precise.map(|rank| rank.3.0),
            latest_observed_at: self.latest_observed_at,
        }
    }
}

/// Builds a stable regional map from acquired evidence only.
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
#[must_use]
pub fn assess_geological_knowledge(
    state: &GeologicalKnowledgeState,
    region: VoxelBounds,
    material: MaterialId,
) -> GeologicalKnowledgeAssessment {
    let mut aggregate = GeologicalEvidenceAggregate::new(region);

    for id in state.observation_ids_for_material(material) {
        let record = state.get_observation(id).unwrap_or_else(|| {
            panic!(
                "runtime invariant broken: geological material index references missing observation {}",
                id.value()
            )
        });
        let Some(overlap) = record.region().intersection(region) else {
            continue;
        };
        let finding = record.finding(material).unwrap_or_else(|| {
            panic!(
                "runtime invariant broken: geological material index references observation without material {}",
                material.value()
            )
        });
        aggregate.add(id, record, finding, overlap);
    }
    aggregate.into_assessment(material, region)
}
