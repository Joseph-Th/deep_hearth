//! Canonical recording of resolved prospecting evidence; sibling knowledge state owns persistence
//! and assessment while physical tools, sampling, labor, and instrument models remain separate.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::material::MaterialId;
use crate::registry::Registries;
use crate::spatial::VoxelBounds;

use super::knowledge::{
    GeologicalEvidenceKind, GeologicalObservationId, GeologicalObservationRecord,
    MaterialAbundanceEstimate,
};

/// Immutable result of a future physical prospecting or analytical resolver.
///
/// There is deliberately no public constructor. Surface inspection, panning, sampling, drilling,
/// assays, and geophysical instruments must resolve their own spatial and abundance uncertainty
/// before they can authorize persistent knowledge.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProspectingResolution {
    region: VoxelBounds,
    evidence: GeologicalEvidenceKind,
    findings: Vec<MaterialAbundanceEstimate>,
}

impl ProspectingResolution {
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
}

/// Failure while validating a resolved observation before it becomes durable knowledge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecordProspectingError {
    NoFindings,
    FindingsNotCanonical {
        previous: MaterialId,
        current: MaterialId,
    },
    UnknownMaterial {
        material: MaterialId,
    },
    ObservationIdExhausted,
    RevisionExhausted,
}

impl Display for RecordProspectingError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoFindings => {
                formatter.write_str("resolved prospecting evidence has no findings")
            }
            Self::FindingsNotCanonical { previous, current } => write!(
                formatter,
                "resolved prospecting findings are not strictly ordered: material {} before {}",
                previous.value(),
                current.value()
            ),
            Self::UnknownMaterial { material } => write!(
                formatter,
                "resolved prospecting evidence references unknown material {}",
                material.value()
            ),
            Self::ObservationIdExhausted => {
                formatter.write_str("geological observation identifier space is exhausted")
            }
            Self::RevisionExhausted => {
                formatter.write_str("geological knowledge revision space is exhausted")
            }
        }
    }
}

impl Error for RecordProspectingError {}

/// Failure to commit an observation after geological knowledge changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProspectingCommitError {
    StaleKnowledgeRevision { expected: u64, actual: u64 },
}

impl Display for ProspectingCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleKnowledgeRevision { expected, actual } => write!(
                formatter,
                "validated prospecting observation expected knowledge revision {expected} but current revision is {actual}"
            ),
        }
    }
}

impl Error for ProspectingCommitError {}

/// Consumed proof that resolved geological evidence can be persisted atomically.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedGeologicalObservation {
    expected_revision: u64,
    next_revision: u64,
    id: GeologicalObservationId,
    next_observation_id: u32,
    region: VoxelBounds,
    evidence: GeologicalEvidenceKind,
    findings: Vec<MaterialAbundanceEstimate>,
    observed_at: SimulationTick,
}

impl ValidatedGeologicalObservation {
    pub fn commit(
        self,
        state: &mut AppState,
    ) -> Result<GeologicalObservationId, ProspectingCommitError> {
        let Self {
            expected_revision,
            next_revision,
            id,
            next_observation_id,
            region,
            evidence,
            findings,
            observed_at,
        } = self;
        let knowledge = state.geological_knowledge_state_mut();
        if knowledge.revision != expected_revision {
            return Err(ProspectingCommitError::StaleKnowledgeRevision {
                expected: expected_revision,
                actual: knowledge.revision,
            });
        }

        for finding in &findings {
            knowledge
                .observations_by_material
                .entry(finding.material())
                .or_default()
                .insert(id);
        }
        let replaced = knowledge.observations.insert(
            id,
            GeologicalObservationRecord {
                id,
                region,
                evidence,
                findings,
                observed_at,
            },
        );
        assert!(
            replaced.is_none(),
            "validated geological observation ID must be unique"
        );
        knowledge.next_observation_id = next_observation_id;
        knowledge.revision = next_revision;
        Ok(id)
    }
}

/// Validates already-resolved prospecting information without consulting hidden deposit truth.
pub fn validate_record_prospecting(
    registries: &Registries,
    state: &AppState,
    resolution: &ProspectingResolution,
) -> Result<ValidatedGeologicalObservation, RecordProspectingError> {
    if resolution.findings.is_empty() {
        return Err(RecordProspectingError::NoFindings);
    }
    for pair in resolution.findings.windows(2) {
        if pair[0].material() >= pair[1].material() {
            return Err(RecordProspectingError::FindingsNotCanonical {
                previous: pair[0].material(),
                current: pair[1].material(),
            });
        }
    }
    for finding in &resolution.findings {
        if registries
            .materials()
            .get_material(finding.material())
            .is_none()
        {
            return Err(RecordProspectingError::UnknownMaterial {
                material: finding.material(),
            });
        }
    }

    let knowledge = state.geological_knowledge();
    let id = GeologicalObservationId::new(knowledge.next_observation_id);
    let Some(next_observation_id) = knowledge.next_observation_id.checked_add(1) else {
        return Err(RecordProspectingError::ObservationIdExhausted);
    };
    let Some(next_revision) = knowledge.revision.checked_add(1) else {
        return Err(RecordProspectingError::RevisionExhausted);
    };

    Ok(ValidatedGeologicalObservation {
        expected_revision: knowledge.revision,
        next_revision,
        id,
        next_observation_id,
        region: resolution.region,
        evidence: resolution.evidence,
        findings: resolution.findings.clone(),
        observed_at: state.tick(),
    })
}

#[cfg(test)]
pub(crate) fn make_test_prospecting_resolution(
    region: VoxelBounds,
    evidence: GeologicalEvidenceKind,
    mut findings: Vec<MaterialAbundanceEstimate>,
) -> ProspectingResolution {
    findings.sort_by_key(|finding| finding.material());
    ProspectingResolution {
        region,
        evidence,
        findings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{MATERIAL_COPPER, MATERIAL_SLAG, build_registries};
    use crate::core::state::validate_loaded_state;
    use crate::core::time::WorldSeed;
    use crate::geology::{
        GeologicalEvidenceConsistency, MaterialAbundanceEstimate, assess_geological_knowledge,
        build_geological_knowledge_map,
    };
    use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
    use crate::simulation::advance_tick;
    use crate::spatial::VoxelCoord;

    fn bounds(min_x: i64, max_x: i64) -> VoxelBounds {
        match VoxelBounds::new(VoxelCoord::new(min_x, -16, 0), VoxelCoord::new(max_x, 0, 8)) {
            Ok(bounds) => bounds,
            Err(error) => panic!("prospecting bounds fixture failed: {error}"),
        }
    }

    fn estimate(material: MaterialId, lower: u32, upper: u32) -> MaterialAbundanceEstimate {
        match MaterialAbundanceEstimate::new(material, lower, upper) {
            Ok(estimate) => estimate,
            Err(error) => panic!("prospecting estimate fixture failed: {error}"),
        }
    }

    fn record(
        registries: &Registries,
        state: &mut AppState,
        resolution: &ProspectingResolution,
    ) -> GeologicalObservationId {
        let token = match validate_record_prospecting(registries, state, resolution) {
            Ok(token) => token,
            Err(error) => panic!("prospecting fixture validation failed: {error}"),
        };
        match token.commit(state) {
            Ok(id) => id,
            Err(error) => panic!("prospecting fixture commit failed: {error}"),
        }
    }

    #[test]
    fn observations_persist_quantitative_uncertainty_without_exposing_deposit_identity() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x6B00_0001));
        let broad = make_test_prospecting_resolution(
            bounds(0, 16),
            GeologicalEvidenceKind::SurfaceExposure,
            vec![estimate(MATERIAL_COPPER, 0, 700_000)],
        );
        let focused = make_test_prospecting_resolution(
            bounds(4, 8),
            GeologicalEvidenceKind::CoreSample,
            vec![estimate(MATERIAL_COPPER, 420_000, 520_000)],
        );
        let broad_id = record(&registries, &mut state, &broad);
        if let Err(error) = advance_tick(&registries, &mut state) {
            panic!("prospecting fixture tick failed: {error}");
        }
        let focused_id = record(&registries, &mut state, &focused);

        let assessment = assess_geological_knowledge(
            state.geological_knowledge(),
            bounds(5, 6),
            MATERIAL_COPPER,
        );
        assert_eq!(assessment.observations(), &[broad_id, focused_id]);
        assert_eq!(
            assessment.consistency(),
            GeologicalEvidenceConsistency::Compatible {
                lower_ppm: 420_000,
                upper_ppm: 520_000,
            }
        );
        assert_eq!(assessment.envelope(), Some((0, 700_000)));
        assert_eq!(assessment.common_evidence_region(), Some(bounds(5, 6)));
        assert_eq!(assessment.most_precise(), Some(focused_id));
        assert_eq!(assessment.latest_observed_at(), Some(state.tick()));
        assert_eq!(state.geology().deposits().count(), 0);
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    }

    #[test]
    fn contradictory_surveys_remain_visible_instead_of_being_averaged() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x6B00_0002));
        let first = make_test_prospecting_resolution(
            bounds(0, 8),
            GeologicalEvidenceKind::MagneticSurvey,
            vec![estimate(MATERIAL_COPPER, 700_000, 900_000)],
        );
        let second = make_test_prospecting_resolution(
            bounds(2, 6),
            GeologicalEvidenceKind::ElectricalSurvey,
            vec![estimate(MATERIAL_COPPER, 100_000, 300_000)],
        );
        record(&registries, &mut state, &first);
        record(&registries, &mut state, &second);

        let assessment = assess_geological_knowledge(
            state.geological_knowledge(),
            bounds(3, 4),
            MATERIAL_COPPER,
        );
        assert_eq!(
            assessment.consistency(),
            GeologicalEvidenceConsistency::Conflicting {
                highest_lower_ppm: 700_000,
                lowest_upper_ppm: 300_000,
            }
        );
        assert_eq!(assessment.envelope(), Some((100_000, 900_000)));
    }

    #[test]
    fn nonoverlapping_observations_do_not_leak_into_local_assessment() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x6B00_0003));
        let remote = make_test_prospecting_resolution(
            bounds(100, 110),
            GeologicalEvidenceKind::SeismicSurvey,
            vec![estimate(MATERIAL_COPPER, 900_000, 1_000_000)],
        );
        record(&registries, &mut state, &remote);

        let assessment = assess_geological_knowledge(
            state.geological_knowledge(),
            bounds(0, 10),
            MATERIAL_COPPER,
        );
        assert_eq!(
            assessment.consistency(),
            GeologicalEvidenceConsistency::NoEvidence
        );
        assert!(assessment.observations().is_empty());
        assert_eq!(assessment.envelope(), None);
        assert_eq!(assessment.common_evidence_region(), None);
    }

    #[test]
    fn disjoint_evidence_inside_a_large_query_is_not_reported_as_a_false_conflict() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x6B00_0007));
        let west = make_test_prospecting_resolution(
            bounds(0, 4),
            GeologicalEvidenceKind::CoreSample,
            vec![estimate(MATERIAL_COPPER, 700_000, 900_000)],
        );
        let east = make_test_prospecting_resolution(
            bounds(6, 10),
            GeologicalEvidenceKind::CoreSample,
            vec![estimate(MATERIAL_COPPER, 100_000, 300_000)],
        );
        record(&registries, &mut state, &west);
        record(&registries, &mut state, &east);

        let assessment = assess_geological_knowledge(
            state.geological_knowledge(),
            bounds(0, 10),
            MATERIAL_COPPER,
        );
        assert_eq!(
            assessment.consistency(),
            GeologicalEvidenceConsistency::SpatiallyIncomparable
        );
        assert_eq!(assessment.common_evidence_region(), None);
        assert_eq!(assessment.envelope(), Some((100_000, 900_000)));
        assert_eq!(assessment.observations().len(), 2);
    }

    #[test]
    fn empty_common_overlap_stays_empty_after_later_evidence() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x6B00_0008));
        for (region, lower, upper) in [
            (bounds(0, 4), 700_000, 900_000),
            (bounds(6, 10), 100_000, 300_000),
            (bounds(2, 8), 400_000, 500_000),
        ] {
            let resolution = make_test_prospecting_resolution(
                region,
                GeologicalEvidenceKind::CoreSample,
                vec![estimate(MATERIAL_COPPER, lower, upper)],
            );
            record(&registries, &mut state, &resolution);
        }

        let assessment = assess_geological_knowledge(
            state.geological_knowledge(),
            bounds(0, 10),
            MATERIAL_COPPER,
        );
        assert_eq!(
            assessment.consistency(),
            GeologicalEvidenceConsistency::SpatiallyIncomparable
        );
        assert_eq!(assessment.common_evidence_region(), None);
        assert_eq!(assessment.observations().len(), 3);
    }

    #[test]
    fn regional_geological_map_is_stable_and_omits_remote_only_materials() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x6B00_0006));
        let local = make_test_prospecting_resolution(
            bounds(0, 8),
            GeologicalEvidenceKind::CoreSample,
            vec![estimate(MATERIAL_COPPER, 400_000, 500_000)],
        );
        let remote = make_test_prospecting_resolution(
            bounds(100, 108),
            GeologicalEvidenceKind::LaboratoryAssay,
            vec![estimate(MATERIAL_SLAG, 700_000, 800_000)],
        );
        record(&registries, &mut state, &local);
        record(&registries, &mut state, &remote);

        let map = build_geological_knowledge_map(state.geological_knowledge(), bounds(2, 4));
        assert_eq!(map.region(), bounds(2, 4));
        assert_eq!(map.assessments().len(), 1);
        assert_eq!(map.assessments()[0].material(), MATERIAL_COPPER);
        assert_eq!(
            map.assessments()[0].consistency(),
            GeologicalEvidenceConsistency::Compatible {
                lower_ppm: 400_000,
                upper_ppm: 500_000,
            }
        );
    }

    #[test]
    fn stale_observation_commit_is_atomic() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x6B00_0004));
        let first = make_test_prospecting_resolution(
            bounds(0, 4),
            GeologicalEvidenceKind::LooseIndicator,
            vec![estimate(MATERIAL_COPPER, 0, 200_000)],
        );
        let second = make_test_prospecting_resolution(
            bounds(4, 8),
            GeologicalEvidenceKind::PannedConcentrate,
            vec![estimate(MATERIAL_SLAG, 100_000, 400_000)],
        );
        let stale = match validate_record_prospecting(&registries, &state, &first) {
            Ok(token) => token,
            Err(error) => panic!("stale prospecting validation failed: {error}"),
        };
        record(&registries, &mut state, &second);
        let before = state.clone();

        assert_eq!(
            stale.commit(&mut state),
            Err(ProspectingCommitError::StaleKnowledgeRevision {
                expected: 0,
                actual: 1,
            })
        );
        assert_eq!(state, before);
    }

    #[test]
    fn prospecting_round_trip_preserves_deterministic_continuation() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x6B00_0005));
        let initial = make_test_prospecting_resolution(
            bounds(0, 16),
            GeologicalEvidenceKind::SurfaceExposure,
            vec![
                estimate(MATERIAL_COPPER, 0, 600_000),
                estimate(MATERIAL_SLAG, 0, 800_000),
            ],
        );
        record(&registries, &mut state, &initial);
        if let Err(error) = advance_tick(&registries, &mut state) {
            panic!("prospecting round-trip tick failed: {error}");
        }

        let encoded = match serde_json::to_vec(&SaveEnvelope::new(&registries, &state)) {
            Ok(encoded) => encoded,
            Err(error) => panic!("prospecting save serialization failed: {error}"),
        };
        let decoded: LoadedSaveEnvelope = match serde_json::from_slice(&encoded) {
            Ok(decoded) => decoded,
            Err(error) => panic!("prospecting save deserialization failed: {error}"),
        };
        let mut loaded = match decoded.into_state(&registries) {
            Ok(loaded) => loaded,
            Err(error) => panic!("prospecting loaded-state validation failed: {error}"),
        };
        assert_eq!(loaded, state);

        let continuation = make_test_prospecting_resolution(
            bounds(2, 6),
            GeologicalEvidenceKind::LaboratoryAssay,
            vec![estimate(MATERIAL_COPPER, 430_000, 450_000)],
        );
        record(&registries, &mut state, &continuation);
        record(&registries, &mut loaded, &continuation);
        assert_eq!(loaded, state);
    }

    fn run_prospecting_soak(seed: WorldSeed) -> AppState {
        let registries = build_registries();
        let mut state = AppState::new(seed);
        for step in 0_u32..2_000 {
            let x = i64::from(step % 64);
            let material = if step.is_multiple_of(2) {
                MATERIAL_COPPER
            } else {
                MATERIAL_SLAG
            };
            let center = (step.wrapping_mul(7919)) % 900_000;
            let lower = center.saturating_sub(25_000);
            let upper = center.saturating_add(25_000).min(1_000_000);
            let resolution = make_test_prospecting_resolution(
                bounds(x, x + 2),
                GeologicalEvidenceKind::CoreSample,
                vec![estimate(material, lower, upper)],
            );
            record(&registries, &mut state, &resolution);
            if let Err(error) = advance_tick(&registries, &mut state) {
                panic!("prospecting soak tick failed at step {step}: {error}");
            }
            if step.is_multiple_of(97)
                && let Err(error) = validate_loaded_state(&registries, &state)
            {
                panic!("prospecting soak exhaustive audit failed at step {step}: {error}");
            }
        }
        assert_eq!(state.geological_knowledge().observations().count(), 2_000);
        assert_eq!(
            state
                .geological_knowledge()
                .observation_ids_for_material(MATERIAL_COPPER)
                .count(),
            1_000
        );
        assert_eq!(
            state
                .geological_knowledge()
                .observation_ids_for_material(MATERIAL_SLAG)
                .count(),
            1_000
        );
        state
    }

    #[test]
    fn prospecting_soak_preserves_indexes_persistence_invariants_and_replay() {
        let seed = WorldSeed::new(0x6B00_5000);
        let first = run_prospecting_soak(seed);
        let second = run_prospecting_soak(seed);
        assert_eq!(first, second);
    }
}
