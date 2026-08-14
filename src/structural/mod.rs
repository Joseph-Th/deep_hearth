//! Structural load and failure subsystem; definitions classify material response, state owns support graphs, analysis resolves loads, and execution commits consequences.

mod analysis;
mod definitions;
mod load;
mod state;
mod structural_execution;

pub use analysis::{
    StructuralAnalysis, StructuralAnalysisError, StructuralAssessment, StructuralDamageEvent,
    StructuralFailureCause, StructuralStage, analyze_structure,
};
pub use definitions::{
    STRUCTURAL_PARTS_PER_MILLION, StructuralLoadMode, StructuralProfileDefinition,
    StructuralProfileId, StructuralRegistry,
};
pub use load::{
    calculate_aggregate_weight_force_ceiling, calculate_pressure_force_ceiling,
    calculate_weight_force_ceiling,
};
pub use state::{
    StructuralElementId, StructuralElementRecord, StructuralLifecycle, StructuralLoadKind,
    StructureState, StructureValidationError,
};
pub use structural_execution::{
    AddStructuralElementError, StructuralCommitError, StructuralMutationError,
    StructuralMutationOutcome, ValidatedStructuralMutation, add_structural_element,
    validate_activate_structural_element, validate_link_support,
    validate_remove_structural_element, validate_remove_support, validate_set_structural_load,
};

pub(crate) use state::validate_loaded_structure;
pub(crate) use structural_execution::validate_set_owned_structural_load;
