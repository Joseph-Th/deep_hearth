//! Trusted-load invariant validation for persistent geological knowledge.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::time::SimulationTick;
use crate::material::{MaterialId, MaterialRegistry};

use super::{
    GeologicalKnowledgeState, GeologicalObservationId, GeologicalObservationRecord,
    PARTS_PER_MILLION, total_lower_bound_ppm,
};

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
    ImpossibleLowerBoundTotal {
        observation: GeologicalObservationId,
        total_ppm: u64,
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
            Self::ImpossibleLowerBoundTotal {
                observation,
                total_ppm,
            } => write!(
                formatter,
                "geological observation {} has combined lower abundance bounds of {total_ppm} ppm, exceeding {PARTS_PER_MILLION} ppm",
                observation.value(),
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
