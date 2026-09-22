//! Typed trusted-load failures for persistent geological knowledge.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Mass, Pressure};
use crate::core::time::SimulationTick;
use crate::geology::GeologicalDepositId;
use crate::material::MaterialId;

use super::super::{GeologicalEvidenceKind, GeologicalObservationId, PARTS_PER_MILLION};

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
    ExcavationHardnessUnsupportedEvidence {
        observation: GeologicalObservationId,
        evidence: GeologicalEvidenceKind,
    },
    ExcavationHardnessAmbiguousFindings {
        observation: GeologicalObservationId,
        count: usize,
    },
    ExcavationHardnessWithoutDefinitePresence {
        observation: GeologicalObservationId,
        material: MaterialId,
    },
    ExcavationHardnessContradictsLiveDeposit {
        observation: GeologicalObservationId,
        deposit: GeologicalDepositId,
        lower: Pressure,
        upper: Pressure,
        actual: Pressure,
    },
    ExcavationHardnessCannotMatchHistoricalDeposit {
        observation: GeologicalObservationId,
        material: MaterialId,
        lower: Pressure,
        upper: Pressure,
    },
    ObservationCannotMatchAuthoredMethod {
        observation: GeologicalObservationId,
        evidence: GeologicalEvidenceKind,
    },
    AbundanceContradictsLiveDeposit {
        observation: GeologicalObservationId,
        deposit: GeologicalDepositId,
        material: MaterialId,
        lower_ppm: u32,
        upper_ppm: u32,
        actual_ppm: u32,
    },
    ResourceMassUnsupportedEvidence {
        observation: GeologicalObservationId,
        evidence: GeologicalEvidenceKind,
    },
    ResourceMassAmbiguousFindings {
        observation: GeologicalObservationId,
        count: usize,
    },
    ResourceMassWithoutDefinitePresence {
        observation: GeologicalObservationId,
        material: MaterialId,
    },
    ResourceMassCannotMatchHistoricalDeposit {
        observation: GeologicalObservationId,
        material: MaterialId,
        lower: Mass,
        upper: Mass,
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
            Self::ExcavationHardnessUnsupportedEvidence {
                observation,
                evidence,
            } => write!(
                formatter,
                "geological observation {} attaches excavation hardness to unsupported {evidence:?} evidence",
                observation.value()
            ),
            Self::ExcavationHardnessAmbiguousFindings { observation, count } => write!(
                formatter,
                "geological observation {} attaches one excavation-hardness band to {count} material findings",
                observation.value()
            ),
            Self::ExcavationHardnessWithoutDefinitePresence {
                observation,
                material,
            } => write!(
                formatter,
                "geological observation {} attaches excavation hardness while material {} may be absent",
                observation.value(),
                material.value()
            ),
            Self::ExcavationHardnessContradictsLiveDeposit {
                observation,
                deposit,
                lower,
                upper,
                actual,
            } => write!(
                formatter,
                "geological observation {} records excavation hardness {}..{} Pa but live matching deposit {} has {} Pa",
                observation.value(),
                lower.pascals(),
                upper.pascals(),
                deposit.value(),
                actual.pascals()
            ),
            Self::ExcavationHardnessCannotMatchHistoricalDeposit {
                observation,
                material,
                lower,
                upper,
            } => write!(
                formatter,
                "geological observation {} records excavation hardness {}..{} Pa for material {} but no deposit that existed at the observation tick could have produced that physical sample",
                observation.value(),
                lower.pascals(),
                upper.pascals(),
                material.value()
            ),
            Self::ObservationCannotMatchAuthoredMethod {
                observation,
                evidence,
            } => write!(
                formatter,
                "geological observation {} carries {:?} evidence whose footprint, uncertainty, or physical metadata cannot be produced by any authored prospecting method",
                observation.value(),
                evidence
            ),
            Self::AbundanceContradictsLiveDeposit {
                observation,
                deposit,
                material,
                lower_ppm,
                upper_ppm,
                actual_ppm,
            } => write!(
                formatter,
                "geological observation {} records material {} abundance {}..{} ppm but live matching deposit {} has {} ppm",
                observation.value(),
                material.value(),
                lower_ppm,
                upper_ppm,
                deposit.value(),
                actual_ppm
            ),
            Self::ResourceMassUnsupportedEvidence {
                observation,
                evidence,
            } => write!(
                formatter,
                "geological observation {} attaches resource mass to unsupported {evidence:?} evidence",
                observation.value()
            ),
            Self::ResourceMassAmbiguousFindings { observation, count } => write!(
                formatter,
                "geological observation {} attaches one resource-mass band to {count} material findings",
                observation.value()
            ),
            Self::ResourceMassWithoutDefinitePresence {
                observation,
                material,
            } => write!(
                formatter,
                "geological observation {} attaches resource mass while material {} may be absent",
                observation.value(),
                material.value()
            ),
            Self::ResourceMassCannotMatchHistoricalDeposit {
                observation,
                material,
                lower,
                upper,
            } => write!(
                formatter,
                "geological observation {} records remaining resource mass {}..{} mg for material {} but no deposit that could have produced that exact-footprint historical observation exists",
                observation.value(),
                lower.milligrams(),
                upper.milligrams(),
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
