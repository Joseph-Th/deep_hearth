//! Finite geological truth, acquired prospecting knowledge, and canonical geological transfers.
//! World generation and advanced survey resolvers remain separate; local field inspection acquires
//! bounded evidence, while mining authorization and timed excavation remain owned by mining.

mod generation_execution;
mod knowledge;
mod prospecting_action;
mod prospecting_execution;
mod state;

pub use generation_execution::{InsertGeneratedDepositError, insert_generated_deposit};
pub use knowledge::{
    AbundanceBound, GeologicalEvidenceConsistency, GeologicalEvidenceKind,
    GeologicalKnowledgeAssessment, GeologicalKnowledgeMap, GeologicalKnowledgeState,
    GeologicalKnowledgeValidationError, GeologicalObservationId, GeologicalObservationRecord,
    MaterialAbundanceEstimate, MaterialAbundanceEstimateError, assess_geological_knowledge,
    build_geological_knowledge_map,
};
pub use prospecting_action::{
    FieldProspectingCommitError, FieldProspectingOutcome, FieldProspectingRequest,
    FieldProspectingStartError, ValidatedFieldProspectingStart, validate_start_field_prospecting,
};
pub use prospecting_execution::{
    ProspectingCommitError, ProspectingResolution, RecordProspectingError,
    ValidatedGeologicalObservation, validate_record_prospecting,
};
pub use state::{
    GeneratedDepositSpec, GeologicalDepositId, GeologicalDepositLifecycle, GeologyValidationError,
};

pub(crate) use knowledge::validate_loaded_geological_knowledge;
pub(crate) use prospecting_action::{
    FieldProspectingTickError, apply_field_prospecting_tick, decide_field_prospecting_tick,
};
pub(crate) use state::{GeologyState, validate_loaded_geology};
