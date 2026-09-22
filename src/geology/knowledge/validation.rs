//! Trusted-load invariant validation for persistent geological knowledge.

use crate::core::time::SimulationTick;
use crate::material::{MaterialId, MaterialRegistry};

use super::{
    ExcavationHardnessContextError, GeologicalKnowledgeState, GeologicalObservationId,
    GeologicalObservationRecord, PARTS_PER_MILLION, ResourceMassContextError,
    total_lower_bound_ppm, validate_excavation_hardness_context, validate_resource_mass_context,
};

mod error;
mod world;

pub use error::GeologicalKnowledgeValidationError;
pub(crate) use world::validate_loaded_geological_evidence_against_world;

pub(crate) fn validate_loaded_geological_knowledge(
    materials: &MaterialRegistry,
    state: &GeologicalKnowledgeState,
    current: SimulationTick,
) -> Result<(), GeologicalKnowledgeValidationError> {
    validate_observation_cursor(state)?;
    for (id, record) in &state.observations {
        validate_observation(materials, state, *id, record, current)?;
    }
    validate_material_observation_index(materials, state)
}

fn validate_observation_cursor(
    state: &GeologicalKnowledgeState,
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
    Ok(())
}

fn validate_observation(
    materials: &MaterialRegistry,
    state: &GeologicalKnowledgeState,
    id: GeologicalObservationId,
    record: &GeologicalObservationRecord,
    current: SimulationTick,
) -> Result<(), GeologicalKnowledgeValidationError> {
    if id.value() == 0 || record.id.value() == 0 {
        return Err(GeologicalKnowledgeValidationError::ZeroObservationId);
    }
    if id != record.id {
        return Err(GeologicalKnowledgeValidationError::IdMismatch {
            key: id,
            record: record.id,
        });
    }
    validate_observation_findings(materials, state, id, record)?;
    validate_excavation_hardness_context(
        record.evidence,
        &record.findings,
        record.excavation_hardness,
    )
    .map_err(|error| match error {
        ExcavationHardnessContextError::UnsupportedEvidence { evidence } => {
            GeologicalKnowledgeValidationError::ExcavationHardnessUnsupportedEvidence {
                observation: id,
                evidence,
            }
        }
        ExcavationHardnessContextError::AmbiguousFindings { count } => {
            GeologicalKnowledgeValidationError::ExcavationHardnessAmbiguousFindings {
                observation: id,
                count,
            }
        }
        ExcavationHardnessContextError::PresenceNotDefinite { material } => {
            GeologicalKnowledgeValidationError::ExcavationHardnessWithoutDefinitePresence {
                observation: id,
                material,
            }
        }
    })?;
    validate_resource_mass_context(record.evidence, &record.findings, record.resource_mass)
        .map_err(|error| match error {
            ResourceMassContextError::UnsupportedEvidence { evidence } => {
                GeologicalKnowledgeValidationError::ResourceMassUnsupportedEvidence {
                    observation: id,
                    evidence,
                }
            }
            ResourceMassContextError::AmbiguousFindings { count } => {
                GeologicalKnowledgeValidationError::ResourceMassAmbiguousFindings {
                    observation: id,
                    count,
                }
            }
            ResourceMassContextError::PresenceNotDefinite { material } => {
                GeologicalKnowledgeValidationError::ResourceMassWithoutDefinitePresence {
                    observation: id,
                    material,
                }
            }
        })?;
    if record.observed_at > current {
        return Err(GeologicalKnowledgeValidationError::ObservedInFuture {
            observation: id,
            observed_at: record.observed_at,
            current,
        });
    }
    Ok(())
}

fn validate_observation_findings(
    materials: &MaterialRegistry,
    state: &GeologicalKnowledgeState,
    id: GeologicalObservationId,
    record: &GeologicalObservationRecord,
) -> Result<(), GeologicalKnowledgeValidationError> {
    if record.findings.is_empty() {
        return Err(GeologicalKnowledgeValidationError::EmptyFindings { observation: id });
    }
    for pair in record.findings.windows(2) {
        if pair[0].material() >= pair[1].material() {
            return Err(GeologicalKnowledgeValidationError::FindingsNotCanonical {
                observation: id,
                previous: pair[0].material(),
                current: pair[1].material(),
            });
        }
    }
    let total_lower_ppm = total_lower_bound_ppm(&record.findings);
    if total_lower_ppm > u64::from(PARTS_PER_MILLION) {
        return Err(
            GeologicalKnowledgeValidationError::ImpossibleLowerBoundTotal {
                observation: id,
                total_ppm: total_lower_ppm,
            },
        );
    }
    for finding in &record.findings {
        validate_observation_finding(materials, state, id, finding.material())?;
    }
    Ok(())
}

fn validate_observation_finding(
    materials: &MaterialRegistry,
    state: &GeologicalKnowledgeState,
    observation: GeologicalObservationId,
    material: MaterialId,
) -> Result<(), GeologicalKnowledgeValidationError> {
    if materials.get_material(material).is_none() {
        return Err(GeologicalKnowledgeValidationError::UnknownFindingMaterial {
            observation,
            material,
        });
    }
    if !state
        .observations_by_material
        .get(&material)
        .is_some_and(|ids| ids.contains(&observation))
    {
        return Err(
            GeologicalKnowledgeValidationError::MissingMaterialIndexEntry {
                observation,
                material,
            },
        );
    }
    Ok(())
}

fn validate_material_observation_index(
    materials: &MaterialRegistry,
    state: &GeologicalKnowledgeState,
) -> Result<(), GeologicalKnowledgeValidationError> {
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
