//! Finite geological truth, acquired prospecting knowledge, and canonical extraction into inventory;
//! world generation, physical survey resolvers, mining authorization, and excavation remain separate.

mod extraction_execution;
mod knowledge;
mod prospecting_execution;
mod state;

pub use extraction_execution::{
    ExtractionResolution, GeologicalExtractionCommitError, GeologicalExtractionError,
    GeologicalExtractionOutcome, InsertGeneratedDepositError, ValidatedGeologicalExtraction,
    insert_generated_deposit, validate_geological_extraction,
};
pub use knowledge::{
    AbundanceBound, GeologicalEvidenceConsistency, GeologicalEvidenceKind,
    GeologicalKnowledgeAssessment, GeologicalKnowledgeMap, GeologicalKnowledgeState,
    GeologicalKnowledgeValidationError, GeologicalObservationId, GeologicalObservationRecord,
    MaterialAbundanceEstimate, MaterialAbundanceEstimateError, assess_geological_knowledge,
    build_geological_knowledge_map,
};
pub use prospecting_execution::{
    ProspectingCommitError, ProspectingResolution, RecordProspectingError,
    ValidatedGeologicalObservation, validate_record_prospecting,
};
pub use state::{GeneratedDepositSpec, GeologicalDepositId, GeologyValidationError};

pub(crate) use knowledge::validate_loaded_geological_knowledge;
pub(crate) use state::{GeologyState, validate_loaded_geology};
