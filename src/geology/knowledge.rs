//! Persistent geological observations and conservative read-only knowledge assessment; sibling
//! prospecting execution owns mutation while authoritative deposits remain separate hidden truth.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Deserializer, Serialize};

use crate::core::time::SimulationTick;
use crate::material::{MaterialId, MaterialRegistry};
use crate::spatial::VoxelBounds;

const PARTS_PER_MILLION: u32 = 1_000_000;

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

/// Persisted evidence acquired at one point in simulation history.
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

    #[must_use]
    pub const fn most_precise(&self) -> Option<GeologicalObservationId> {
        self.most_precise
    }

    #[must_use]
    pub const fn latest_observed_at(&self) -> Option<SimulationTick> {
        self.latest_observed_at
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
            match assessment.consistency() {
                GeologicalEvidenceConsistency::NoEvidence => None,
                GeologicalEvidenceConsistency::SpatiallyIncomparable => Some(assessment),
                GeologicalEvidenceConsistency::Compatible {
                    lower_ppm: _lower_ppm,
                    upper_ppm: _upper_ppm,
                } => Some(assessment),
                GeologicalEvidenceConsistency::Conflicting {
                    highest_lower_ppm: _highest_lower_ppm,
                    lowest_upper_ppm: _lowest_upper_ppm,
                } => Some(assessment),
            }
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
    let mut observations = Vec::new();
    let mut highest_lower = 0_u32;
    let mut lowest_upper = PARTS_PER_MILLION;
    let mut envelope_lower = PARTS_PER_MILLION;
    let mut envelope_upper = 0_u32;
    let mut common_evidence_region = Some(region);
    let mut most_precise: Option<(u32, u128, SimulationTick, GeologicalObservationId)> = None;
    let mut latest = None;

    for id in state.observation_ids_for_material(material) {
        let record = match state.get_observation(id) {
            Some(record) => record,
            None => panic!(
                "runtime invariant broken: geological material index references missing observation {}",
                id.value()
            ),
        };
        if !record.region().has_intersection(region) {
            continue;
        }
        let overlap = match record.region().intersection(region) {
            Some(overlap) => overlap,
            None => panic!("intersecting geological bounds must have a nonempty intersection"),
        };
        common_evidence_region =
            common_evidence_region.and_then(|common| common.intersection(overlap));
        let finding = match record.finding(material) {
            Some(finding) => finding,
            None => panic!(
                "runtime invariant broken: geological material index references observation without material {}",
                material.value()
            ),
        };
        observations.push(id);
        highest_lower = highest_lower.max(finding.lower_ppm());
        lowest_upper = lowest_upper.min(finding.upper_ppm());
        envelope_lower = envelope_lower.min(finding.lower_ppm());
        envelope_upper = envelope_upper.max(finding.upper_ppm());
        latest = Some(match latest {
            Some(current) => std::cmp::max(current, record.observed_at()),
            None => record.observed_at(),
        });

        let volume = record.region().voxel_count().unwrap_or(u128::MAX);
        let candidate = (finding.width_ppm(), volume, record.observed_at(), id);
        let replace = match most_precise {
            None => true,
            Some(current) => {
                candidate.0 < current.0
                    || (candidate.0 == current.0 && candidate.1 < current.1)
                    || (candidate.0 == current.0
                        && candidate.1 == current.1
                        && candidate.2 > current.2)
                    || (candidate.0 == current.0
                        && candidate.1 == current.1
                        && candidate.2 == current.2
                        && candidate.3 < current.3)
            }
        };
        if replace {
            most_precise = Some(candidate);
        }
    }

    if observations.is_empty() {
        common_evidence_region = None;
    }
    let (consistency, envelope) = if observations.is_empty() {
        (GeologicalEvidenceConsistency::NoEvidence, None)
    } else if common_evidence_region.is_none() {
        (
            GeologicalEvidenceConsistency::SpatiallyIncomparable,
            Some((envelope_lower, envelope_upper)),
        )
    } else if highest_lower <= lowest_upper {
        (
            GeologicalEvidenceConsistency::Compatible {
                lower_ppm: highest_lower,
                upper_ppm: lowest_upper,
            },
            Some((envelope_lower, envelope_upper)),
        )
    } else {
        (
            GeologicalEvidenceConsistency::Conflicting {
                highest_lower_ppm: highest_lower,
                lowest_upper_ppm: lowest_upper,
            },
            Some((envelope_lower, envelope_upper)),
        )
    };

    GeologicalKnowledgeAssessment {
        material,
        region,
        observations,
        consistency,
        envelope,
        common_evidence_region,
        most_precise: most_precise.map(|candidate| candidate.3),
        latest_observed_at: latest,
    }
}

/// Persistent invariant failure for acquired geological knowledge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GeologicalKnowledgeValidationError {
    ZeroNextObservationId,
    NextIdNotAfterExisting {
        next: u32,
        highest: GeologicalObservationId,
    },
    ZeroObservationId,
    IdMismatch {
        key: GeologicalObservationId,
        record: GeologicalObservationId,
    },
    EmptyFindings {
        observation: GeologicalObservationId,
    },
    FindingsNotCanonical {
        observation: GeologicalObservationId,
        previous: MaterialId,
        current: MaterialId,
    },
    UnknownFindingMaterial {
        observation: GeologicalObservationId,
        material: MaterialId,
    },
    ObservedInFuture {
        observation: GeologicalObservationId,
        observed_at: SimulationTick,
        current: SimulationTick,
    },
    MissingMaterialIndexEntry {
        observation: GeologicalObservationId,
        material: MaterialId,
    },
    UnknownIndexedMaterial {
        material: MaterialId,
    },
    EmptyMaterialIndex {
        material: MaterialId,
    },
    UnknownIndexedObservation {
        material: MaterialId,
        observation: GeologicalObservationId,
    },
    IndexMaterialMismatch {
        material: MaterialId,
        observation: GeologicalObservationId,
    },
}

impl Display for GeologicalKnowledgeValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroNextObservationId => {
                formatter.write_str("next geological observation id must not be zero")
            }
            Self::NextIdNotAfterExisting { next, highest } => write!(
                formatter,
                "next geological observation id {next} is not after existing id {}",
                highest.value()
            ),
            Self::ZeroObservationId => {
                formatter.write_str("geological observation id must not be zero")
            }
            Self::IdMismatch { key, record } => write!(
                formatter,
                "geological observation map key {} disagrees with record id {}",
                key.value(),
                record.value()
            ),
            Self::EmptyFindings { observation } => write!(
                formatter,
                "geological observation {} contains no material findings",
                observation.value()
            ),
            Self::FindingsNotCanonical {
                observation,
                previous,
                current,
            } => write!(
                formatter,
                "geological observation {} findings are not strictly ordered: material {} before {}",
                observation.value(),
                previous.value(),
                current.value()
            ),
            Self::UnknownFindingMaterial {
                observation,
                material,
            } => write!(
                formatter,
                "geological observation {} references unknown material {}",
                observation.value(),
                material.value()
            ),
            Self::ObservedInFuture {
                observation,
                observed_at,
                current,
            } => write!(
                formatter,
                "geological observation {} was recorded at tick {} after current tick {}",
                observation.value(),
                observed_at.value(),
                current.value()
            ),
            Self::MissingMaterialIndexEntry {
                observation,
                material,
            } => write!(
                formatter,
                "geological observation {} material {} is missing from the material index",
                observation.value(),
                material.value()
            ),
            Self::UnknownIndexedMaterial { material } => write!(
                formatter,
                "geological material index references unknown material {}",
                material.value()
            ),
            Self::EmptyMaterialIndex { material } => write!(
                formatter,
                "geological material {} has an empty observation index",
                material.value()
            ),
            Self::UnknownIndexedObservation {
                material,
                observation,
            } => write!(
                formatter,
                "geological material {} index references missing observation {}",
                material.value(),
                observation.value()
            ),
            Self::IndexMaterialMismatch {
                material,
                observation,
            } => write!(
                formatter,
                "geological material {} index references observation {} without that finding",
                material.value(),
                observation.value()
            ),
        }
    }
}

impl Error for GeologicalKnowledgeValidationError {}

pub(crate) fn validate_loaded_geological_knowledge(
    materials: &MaterialRegistry,
    state: &GeologicalKnowledgeState,
    current: SimulationTick,
) -> Result<(), GeologicalKnowledgeValidationError> {
    if state.next_observation_id == 0 {
        return Err(GeologicalKnowledgeValidationError::ZeroNextObservationId);
    }
    if let Some(highest) = state.observations.keys().next_back().copied()
        && state.next_observation_id <= highest.value()
    {
        return Err(GeologicalKnowledgeValidationError::NextIdNotAfterExisting {
            next: state.next_observation_id,
            highest,
        });
    }

    for (id, record) in &state.observations {
        if id.value() == 0 || record.id.value() == 0 {
            return Err(GeologicalKnowledgeValidationError::ZeroObservationId);
        }
        if *id != record.id {
            return Err(GeologicalKnowledgeValidationError::IdMismatch {
                key: *id,
                record: record.id,
            });
        }
        if record.findings.is_empty() {
            return Err(GeologicalKnowledgeValidationError::EmptyFindings { observation: *id });
        }
        for pair in record.findings.windows(2) {
            if pair[0].material() >= pair[1].material() {
                return Err(GeologicalKnowledgeValidationError::FindingsNotCanonical {
                    observation: *id,
                    previous: pair[0].material(),
                    current: pair[1].material(),
                });
            }
        }
        for finding in &record.findings {
            let material = finding.material();
            if materials.get_material(material).is_none() {
                return Err(GeologicalKnowledgeValidationError::UnknownFindingMaterial {
                    observation: *id,
                    material,
                });
            }
            if !state
                .observations_by_material
                .get(&material)
                .is_some_and(|ids| ids.contains(id))
            {
                return Err(
                    GeologicalKnowledgeValidationError::MissingMaterialIndexEntry {
                        observation: *id,
                        material,
                    },
                );
            }
        }
        if record.observed_at > current {
            return Err(GeologicalKnowledgeValidationError::ObservedInFuture {
                observation: *id,
                observed_at: record.observed_at,
                current,
            });
        }
    }

    for (material, ids) in &state.observations_by_material {
        if materials.get_material(*material).is_none() {
            return Err(GeologicalKnowledgeValidationError::UnknownIndexedMaterial {
                material: *material,
            });
        }
        if ids.is_empty() {
            return Err(GeologicalKnowledgeValidationError::EmptyMaterialIndex {
                material: *material,
            });
        }
        for id in ids {
            let record = state.observations.get(id).ok_or(
                GeologicalKnowledgeValidationError::UnknownIndexedObservation {
                    material: *material,
                    observation: *id,
                },
            )?;
            if record.finding(*material).is_none() {
                return Err(GeologicalKnowledgeValidationError::IndexMaterialMismatch {
                    material: *material,
                    observation: *id,
                });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{MATERIAL_COPPER, MATERIAL_SLAG, build_registries};
    use crate::spatial::VoxelCoord;

    fn bounds() -> VoxelBounds {
        match VoxelBounds::new(VoxelCoord::new(0, -8, 0), VoxelCoord::new(4, -4, 4)) {
            Ok(bounds) => bounds,
            Err(error) => panic!("geological knowledge bounds fixture failed: {error}"),
        }
    }

    fn estimate(material: MaterialId, lower: u32, upper: u32) -> MaterialAbundanceEstimate {
        match MaterialAbundanceEstimate::new(material, lower, upper) {
            Ok(estimate) => estimate,
            Err(error) => panic!("geological knowledge estimate fixture failed: {error}"),
        }
    }

    #[test]
    fn abundance_estimate_rejects_invalid_fraction_bounds() {
        assert_eq!(
            MaterialAbundanceEstimate::new(MATERIAL_COPPER, 700_000, 600_000),
            Err(MaterialAbundanceEstimateError::InvertedBounds {
                lower_ppm: 700_000,
                upper_ppm: 600_000,
            })
        );
        assert_eq!(
            MaterialAbundanceEstimate::new(MATERIAL_COPPER, 0, 1_000_001),
            Err(MaterialAbundanceEstimateError::AboveUnity {
                bound: AbundanceBound::Upper,
                value: 1_000_001,
            })
        );
    }

    #[test]
    fn loaded_validation_rejects_missing_material_index_entry() {
        let registries = build_registries();
        let id = GeologicalObservationId::new(1);
        let mut state = GeologicalKnowledgeState::new();
        state.next_observation_id = 2;
        state.observations.insert(
            id,
            GeologicalObservationRecord {
                id,
                region: bounds(),
                evidence: GeologicalEvidenceKind::CoreSample,
                findings: vec![estimate(MATERIAL_COPPER, 600_000, 800_000)],
                observed_at: SimulationTick::ZERO,
            },
        );

        assert_eq!(
            validate_loaded_geological_knowledge(
                registries.materials(),
                &state,
                SimulationTick::ZERO,
            ),
            Err(
                GeologicalKnowledgeValidationError::MissingMaterialIndexEntry {
                    observation: id,
                    material: MATERIAL_COPPER,
                }
            )
        );
    }

    #[test]
    fn loaded_validation_rejects_noncanonical_duplicate_material_findings() {
        let registries = build_registries();
        let id = GeologicalObservationId::new(1);
        let mut state = GeologicalKnowledgeState::new();
        state.next_observation_id = 2;
        state.observations.insert(
            id,
            GeologicalObservationRecord {
                id,
                region: bounds(),
                evidence: GeologicalEvidenceKind::LaboratoryAssay,
                findings: vec![
                    estimate(MATERIAL_COPPER, 500_000, 600_000),
                    estimate(MATERIAL_COPPER, 550_000, 650_000),
                ],
                observed_at: SimulationTick::ZERO,
            },
        );
        state
            .observations_by_material
            .insert(MATERIAL_COPPER, BTreeSet::from([id]));

        assert_eq!(
            validate_loaded_geological_knowledge(
                registries.materials(),
                &state,
                SimulationTick::ZERO,
            ),
            Err(GeologicalKnowledgeValidationError::FindingsNotCanonical {
                observation: id,
                previous: MATERIAL_COPPER,
                current: MATERIAL_COPPER,
            })
        );
    }

    #[test]
    fn material_index_validation_checks_both_directions() {
        let registries = build_registries();
        let id = GeologicalObservationId::new(1);
        let mut state = GeologicalKnowledgeState::new();
        state.next_observation_id = 2;
        state.observations.insert(
            id,
            GeologicalObservationRecord {
                id,
                region: bounds(),
                evidence: GeologicalEvidenceKind::MagneticSurvey,
                findings: vec![estimate(MATERIAL_COPPER, 100_000, 900_000)],
                observed_at: SimulationTick::ZERO,
            },
        );
        state
            .observations_by_material
            .insert(MATERIAL_COPPER, BTreeSet::from([id]));
        state
            .observations_by_material
            .insert(MATERIAL_SLAG, BTreeSet::from([id]));

        assert_eq!(
            validate_loaded_geological_knowledge(
                registries.materials(),
                &state,
                SimulationTick::ZERO,
            ),
            Err(GeologicalKnowledgeValidationError::IndexMaterialMismatch {
                material: MATERIAL_SLAG,
                observation: id,
            })
        );
    }
}
