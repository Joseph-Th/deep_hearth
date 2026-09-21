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
    MaterialAbundanceEstimate, MaterialAbundanceEstimateError, ResourceMassEstimate,
    ResourceMassEstimateError,
};
pub use state::GeologicalKnowledgeState;
pub use validation::GeologicalKnowledgeValidationError;

pub(super) use observation::{
    ExcavationHardnessContextError, PARTS_PER_MILLION, ResourceMassContextError,
    total_lower_bound_ppm, validate_excavation_hardness_context, validate_resource_mass_context,
};
pub(crate) use validation::{
    validate_loaded_geological_knowledge, validate_loaded_hardness_against_live_geology,
};

#[cfg(test)]
#[path = "knowledge_tests.rs"]
mod tests;
