//! Records resolved prospecting evidence into persistent geological knowledge.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::material::MaterialId;
use crate::registry::Registries;
use crate::spatial::VoxelBounds;

use super::knowledge::{
    ExcavationHardnessContextError, ExcavationHardnessEstimate, GeologicalEvidenceKind,
    GeologicalObservationId, GeologicalObservationRecord, MaterialAbundanceEstimate,
    PARTS_PER_MILLION, total_lower_bound_ppm, validate_excavation_hardness_context,
};

/// Immutable evidence result produced by an authorized prospecting or analytical resolver.
///
/// Runtime field prospecting constructs this internally after its timed labor action completes. Test
/// code can construct synthetic evidence for knowledge-boundary coverage. Future panning, sampling,
/// drilling, assays, and geophysics must resolve their own spatial and abundance uncertainty before
/// they can authorize persistent knowledge.
#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ProspectingResolution {
    region: VoxelBounds,
    evidence: GeologicalEvidenceKind,
    findings: Vec<MaterialAbundanceEstimate>,
    excavation_hardness: Option<ExcavationHardnessEstimate>,
}

impl ProspectingResolution {
    pub(super) fn new_runtime(
        region: VoxelBounds,
        evidence: GeologicalEvidenceKind,
        mut findings: Vec<MaterialAbundanceEstimate>,
        excavation_hardness: Option<ExcavationHardnessEstimate>,
    ) -> Self {
        findings.sort_by_key(|finding| finding.material());
        Self {
            region,
            evidence,
            findings,
            excavation_hardness,
        }
    }

    /// Constructs deliberately synthetic acquired evidence for controlled tests.
    #[cfg(test)]
    pub(crate) fn new_for_fixture(
        region: VoxelBounds,
        evidence: GeologicalEvidenceKind,
        mut findings: Vec<MaterialAbundanceEstimate>,
    ) -> Self {
        findings.sort_by_key(|finding| finding.material());
        Self {
            region,
            evidence,
            findings,
            excavation_hardness: None,
        }
    }

    /// Adds deliberately synthetic acquired hardness evidence for focused knowledge/mining tests.
    #[cfg(test)]
    pub(crate) fn with_excavation_hardness_for_fixture(
        mut self,
        excavation_hardness: ExcavationHardnessEstimate,
    ) -> Self {
        self.excavation_hardness = Some(excavation_hardness);
        self
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
    ImpossibleLowerBoundTotal {
        total_ppm: u64,
    },
    UnknownMaterial {
        material: MaterialId,
    },
    ExcavationHardnessUnsupportedEvidence {
        evidence: GeologicalEvidenceKind,
    },
    ExcavationHardnessAmbiguousFindings {
        count: usize,
    },
    ExcavationHardnessWithoutDefinitePresence {
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
            Self::ImpossibleLowerBoundTotal { total_ppm } => write!(
                formatter,
                "resolved prospecting findings have combined lower abundance bounds of {total_ppm} ppm, exceeding {PARTS_PER_MILLION} ppm"
            ),
            Self::UnknownMaterial { material } => write!(
                formatter,
                "resolved prospecting evidence references unknown material {}",
                material.value()
            ),
            Self::ExcavationHardnessUnsupportedEvidence { evidence } => write!(
                formatter,
                "resolved prospecting evidence attaches excavation hardness to unsupported {evidence:?} evidence"
            ),
            Self::ExcavationHardnessAmbiguousFindings { count } => write!(
                formatter,
                "resolved prospecting evidence attaches one excavation-hardness band to {count} material findings"
            ),
            Self::ExcavationHardnessWithoutDefinitePresence { material } => write!(
                formatter,
                "resolved prospecting evidence attaches excavation hardness while material {} may be absent",
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

/// Consumed proof that resolved geological evidence can be persisted atomically.
#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub struct ValidatedGeologicalObservation {
    expected_revision: u64,
    next_revision: u64,
    id: GeologicalObservationId,
    next_observation_id: u32,
    region: VoxelBounds,
    evidence: GeologicalEvidenceKind,
    findings: Vec<MaterialAbundanceEstimate>,
    excavation_hardness: Option<ExcavationHardnessEstimate>,
    observed_at: SimulationTick,
}

impl ValidatedGeologicalObservation {
    pub(super) fn apply_prechecked(self, state: &mut AppState) -> GeologicalObservationId {
        let Self {
            expected_revision,
            next_revision,
            id,
            next_observation_id,
            region,
            evidence,
            findings,
            excavation_hardness,
            observed_at,
        } = self;
        let knowledge = state.geological_knowledge_state_mut();
        assert_eq!(
            knowledge.revision(),
            expected_revision,
            "prechecked prospecting observation requires its validated knowledge revision"
        );
        knowledge.insert_observation(
            GeologicalObservationRecord {
                id,
                region,
                evidence,
                findings,
                excavation_hardness,
                observed_at,
            },
            next_observation_id,
            next_revision,
        );
        id
    }
}

/// Validates already-resolved prospecting information without consulting hidden deposit truth.
#[cfg(test)]
pub(crate) fn validate_record_prospecting(
    registries: &Registries,
    state: &AppState,
    resolution: ProspectingResolution,
) -> Result<ValidatedGeologicalObservation, RecordProspectingError> {
    let mut validated =
        validate_record_prospecting_batch_at(registries, state, vec![resolution], state.tick())?;
    Ok(validated
        .pop()
        .unwrap_or_else(|| unreachable!("one prospecting resolution validates to one observation")))
}

/// Records one synthetic observation through the production validation and apply path.
#[cfg(test)]
pub(crate) fn record_prospecting_for_test(
    registries: &Registries,
    state: &mut AppState,
    resolution: ProspectingResolution,
) -> Result<GeologicalObservationId, RecordProspectingError> {
    Ok(validate_record_prospecting(registries, state, resolution)?.apply_prechecked(state))
}

pub(super) fn validate_record_prospecting_batch_at(
    registries: &Registries,
    state: &AppState,
    resolutions: Vec<ProspectingResolution>,
    observed_at: SimulationTick,
) -> Result<Vec<ValidatedGeologicalObservation>, RecordProspectingError> {
    let knowledge = state.geological_knowledge();
    let mut expected_revision = knowledge.revision();
    let mut id_cursor = knowledge.next_observation_id();
    let mut validated = Vec::with_capacity(resolutions.len());

    for resolution in resolutions {
        validate_resolution_findings(registries, &resolution)?;
        let id = GeologicalObservationId::new(id_cursor);
        let Some(next_observation_id) = id_cursor.checked_add(1) else {
            return Err(RecordProspectingError::ObservationIdExhausted);
        };
        let Some(next_revision) = expected_revision.checked_add(1) else {
            return Err(RecordProspectingError::RevisionExhausted);
        };
        let ProspectingResolution {
            region,
            evidence,
            findings,
            excavation_hardness,
        } = resolution;
        validated.push(ValidatedGeologicalObservation {
            expected_revision,
            next_revision,
            id,
            next_observation_id,
            region,
            evidence,
            findings,
            excavation_hardness,
            observed_at,
        });
        expected_revision = next_revision;
        id_cursor = next_observation_id;
    }
    Ok(validated)
}

fn validate_resolution_findings(
    registries: &Registries,
    resolution: &ProspectingResolution,
) -> Result<(), RecordProspectingError> {
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
    let total_lower_ppm = total_lower_bound_ppm(&resolution.findings);
    if total_lower_ppm > u64::from(PARTS_PER_MILLION) {
        return Err(RecordProspectingError::ImpossibleLowerBoundTotal {
            total_ppm: total_lower_ppm,
        });
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
    validate_excavation_hardness_context(
        resolution.evidence,
        &resolution.findings,
        resolution.excavation_hardness,
    )
    .map_err(|error| match error {
        ExcavationHardnessContextError::UnsupportedEvidence { evidence } => {
            RecordProspectingError::ExcavationHardnessUnsupportedEvidence { evidence }
        }
        ExcavationHardnessContextError::AmbiguousFindings { count } => {
            RecordProspectingError::ExcavationHardnessAmbiguousFindings { count }
        }
        ExcavationHardnessContextError::PresenceNotDefinite { material } => {
            RecordProspectingError::ExcavationHardnessWithoutDefinitePresence { material }
        }
    })?;
    Ok(())
}

#[cfg(test)]
#[path = "prospecting_execution_tests.rs"]
mod tests;
