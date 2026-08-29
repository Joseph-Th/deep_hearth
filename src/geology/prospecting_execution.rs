//! Records resolved prospecting evidence into persistent geological knowledge.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::material::MaterialId;
use crate::registry::Registries;
use crate::spatial::VoxelBounds;

use super::knowledge::{
    GeologicalEvidenceKind, GeologicalObservationId, GeologicalObservationRecord,
    MaterialAbundanceEstimate, PARTS_PER_MILLION, total_lower_bound_ppm,
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
}

impl ProspectingResolution {
    pub(super) fn new_runtime(
        region: VoxelBounds,
        evidence: GeologicalEvidenceKind,
        mut findings: Vec<MaterialAbundanceEstimate>,
    ) -> Self {
        findings.sort_by_key(|finding| finding.material());
        Self {
            region,
            evidence,
            findings,
        }
    }

    /// Unit-test constructor for deliberately synthetic or contradictory evidence.
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
        }
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
#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProspectingCommitError {
    StaleKnowledgeRevision { expected: u64, actual: u64 },
}

#[cfg(test)]
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

#[cfg(test)]
impl Error for ProspectingCommitError {}

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
    observed_at: SimulationTick,
}

impl ValidatedGeologicalObservation {
    #[cfg(test)]
    pub(crate) fn commit(
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
        if knowledge.revision() != expected_revision {
            return Err(ProspectingCommitError::StaleKnowledgeRevision {
                expected: expected_revision,
                actual: knowledge.revision(),
            });
        }

        knowledge.insert_observation(
            GeologicalObservationRecord {
                id,
                region,
                evidence,
                findings,
                observed_at,
            },
            next_observation_id,
            next_revision,
        );
        Ok(id)
    }

    pub(super) fn apply_prechecked(self, state: &mut AppState) -> GeologicalObservationId {
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
    validate_record_prospecting_at(registries, state, resolution, state.tick())
}

pub(super) fn validate_record_prospecting_at(
    registries: &Registries,
    state: &AppState,
    resolution: ProspectingResolution,
    observed_at: SimulationTick,
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

    let knowledge = state.geological_knowledge();
    let id = GeologicalObservationId::new(knowledge.next_observation_id());
    let Some(next_observation_id) = knowledge.next_observation_id().checked_add(1) else {
        return Err(RecordProspectingError::ObservationIdExhausted);
    };
    let Some(next_revision) = knowledge.revision().checked_add(1) else {
        return Err(RecordProspectingError::RevisionExhausted);
    };
    let ProspectingResolution {
        region,
        evidence,
        findings,
    } = resolution;

    Ok(ValidatedGeologicalObservation {
        expected_revision: knowledge.revision(),
        next_revision,
        id,
        next_observation_id,
        region,
        evidence,
        findings,
        observed_at,
    })
}

#[cfg(test)]
#[path = "prospecting_execution_tests.rs"]
mod tests;
