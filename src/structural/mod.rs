//! Owns structural definitions, support graphs, load analysis, damage, and canonical mutation.

mod analysis;
mod construction_execution;
mod definitions;
mod element_execution;
mod geometry;
mod load;
mod member_projection;
mod player_construction;
mod player_placement;
mod state;
mod structural_execution;
mod support_index;
#[cfg(test)]
mod test_support;

pub use analysis::{
    StructuralAnalysis, StructuralAnalysisError, StructuralAssessment, StructuralDamageEvent,
    StructuralFailureCause, StructuralStage, analyze_structure, calculate_pristine_member_capacity,
    calculate_structural_utilization_ppm, is_structural_load_within_utilization_limit,
};
pub use definitions::{
    STRUCTURAL_PARTS_PER_MILLION, StructuralLoadMode, StructuralProfileDefinition,
    StructuralProfileId, StructuralRegistry,
};
pub use geometry::{
    StructuralGeometryError, calculate_prismatic_material_mass_ceiling,
    calculate_prismatic_volume_ceiling,
};
pub(crate) use load::calculate_fractional_milligram_weight_force_ceiling;
pub use load::{calculate_aggregate_weight_force_ceiling, calculate_weight_force_ceiling};
pub use member_projection::{
    PrismaticMemberLoadProjection, PrismaticMemberLoadRequest, StructuralMemberLoadProjectionError,
    project_prismatic_member_load,
};
pub use player_construction::{
    PlayerStructuralConstructionCommitError, PlayerStructuralConstructionError,
    ValidatedPlayerStructuralConstructionStart, validate_start_player_structural_construction,
};
pub(crate) use player_construction::{
    StructuralConstructionTickPlan, apply_structural_construction_tick, bounds_are_local,
    decide_structural_construction_tick,
    validate_start_player_structural_construction_for_planned_member,
};
pub use player_placement::{
    PlayerGroundedStructuralConstructionCommitError, PlayerGroundedStructuralConstructionError,
    ValidatedPlayerGroundedStructuralConstructionStart,
    validate_start_player_grounded_structural_construction_from_stack,
};
pub use state::{
    StructuralElementGeometry, StructuralElementId, StructuralElementRecord, StructuralLifecycle,
    StructuralLoadKind, StructureState, StructureValidationError,
};
pub(crate) use structural_execution::StructuralMutationOutcome;
pub use structural_execution::{StructuralCommitError, StructuralMutationError};

#[cfg(any(test, feature = "test-gameplay"))]
pub(crate) use element_execution::add_structural_element;
pub use element_execution::{
    AddStructuralElementError, GroundedStructuralElementPlan,
    PlanPlayerStructuralElementCommitError, PlanPlayerStructuralElementError,
    ValidatedPlayerStructuralElementPlan, validate_plan_player_grounded_structural_element,
};
pub(crate) use structural_execution::ValidatedStructuralMutation;
#[cfg(any(test, feature = "test-gameplay"))]
pub(crate) use structural_execution::validate_activate_structural_element;
pub(crate) use structural_execution::validate_activate_structural_element_in;

#[cfg(test)]
pub(crate) use structural_execution::{
    validate_link_support, validate_remove_structural_element, validate_set_structural_load,
};

#[cfg(feature = "test-gameplay")]
pub(crate) use construction_execution::resolve_structural_material_requirement;
pub use construction_execution::{
    StructuralConstructionBindingError, StructuralConstructionCommitError,
    StructuralConstructionError,
};
pub(crate) use construction_execution::{
    bind_structural_construction_selection, resolve_required_mass,
    validate_structural_construction, validate_structural_construction_for_planned_member,
};
pub(crate) use state::validate_loaded_structure;
pub(crate) use structural_execution::{
    ValidatedStructuralLoadChange, validate_owned_structural_load_change,
};
pub(crate) use support_index::{
    SupportIndexValidationFault, apply_support_index_change, assert_support_index_change_available,
    validate_support_index,
};

#[cfg(test)]
pub(crate) use test_support::{
    make_test_structural_geometry, materialize_structural_element_for_test,
};
