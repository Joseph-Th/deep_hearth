//! Owns persistent geological observations and conservative read-only knowledge assessment.

mod assessment;
mod observation;
mod state;
mod validation;

pub use assessment::{
    GeologicalEvidenceConsistency, GeologicalKnowledgeAssessment, GeologicalKnowledgeMap,
    assess_geological_knowledge, build_geological_knowledge_map,
};
pub use observation::{
    AbundanceBound, ExcavationHardnessEstimate, ExcavationHardnessEstimateError,
    GeologicalEvidenceKind, GeologicalObservationId, GeologicalObservationRecord,
    MaterialAbundanceEstimate, MaterialAbundanceEstimateError,
};
pub use state::GeologicalKnowledgeState;
pub use validation::GeologicalKnowledgeValidationError;

pub(super) use observation::{PARTS_PER_MILLION, total_lower_bound_ppm};
pub(crate) use validation::validate_loaded_geological_knowledge;

#[cfg(test)]
#[path = "knowledge_tests.rs"]
mod tests;
