//! Finite geological truth, acquired prospecting knowledge, and canonical geological transfers.
//! World generation and advanced survey resolvers remain separate; field prospecting acquires
//! bounded evidence, while mining authorization and timed excavation remain owned by mining.

mod coverage;
mod generation_execution;
mod knowledge;
mod material_validation;
mod prospecting_action;
mod prospecting_execution;
mod state;

pub use generation_execution::InsertGeneratedDepositError as GeologicalDepositGenerationError;
#[cfg(test)]
pub(crate) use generation_execution::InsertGeneratedDepositError;
#[cfg(any(test, feature = "test-gameplay"))]
pub(crate) use generation_execution::insert_generated_deposit;
pub(crate) use generation_execution::{
    GenerateGeologicalDepositCommitError, ValidatedGeologicalDepositGeneration,
    validate_generate_geological_deposit,
};
pub use knowledge::{
    AbundanceBound, ExcavationHardnessEstimate, ExcavationHardnessEstimateError,
    GeologicalEvidenceConsistency, GeologicalEvidenceKind, GeologicalKnowledgeAssessment,
    GeologicalKnowledgeMap, GeologicalKnowledgeState, GeologicalKnowledgeValidationError,
    GeologicalObservationId, GeologicalObservationRecord, MaterialAbundanceEstimate,
    MaterialAbundanceEstimateError, ResourceMassEstimate, ResourceMassEstimateError,
    assess_geological_knowledge, build_geological_knowledge_map,
};
pub use prospecting_action::{
    FieldProspectingCommitError, FieldProspectingOutcome, FieldProspectingRequest,
    FieldProspectingStartError, PlayerProspectingInstrumentOption, PlayerProspectingOption,
    ValidatedFieldProspectingStart, player_prospecting_options, validate_start_field_prospecting,
};
#[cfg(test)]
pub(crate) use prospecting_execution::record_prospecting_for_test;
pub(crate) use prospecting_execution::{
    ProspectingResolution, RecordProspectingError, ValidatedGeologicalObservation,
};
pub(crate) use state::GeneratedDepositSpec;
pub use state::{GeologicalDepositId, GeologicalDepositLifecycle, GeologyValidationError};

pub(crate) use knowledge::{
    validate_loaded_geological_evidence_against_world, validate_loaded_geological_knowledge,
};
pub(crate) use prospecting_action::{
    FieldProspectingInterruptionPlan, FieldProspectingTickError,
    apply_field_prospecting_interruption, apply_field_prospecting_tick,
    decide_field_prospecting_interruption, decide_field_prospecting_tick,
};
pub(crate) use state::{GeologyState, validate_loaded_geology};
