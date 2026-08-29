//! Owns structural definitions, support graphs, load analysis, damage, and canonical mutation.

mod analysis;
#[cfg(any(test, feature = "test-gameplay"))]
mod construction_execution;
mod definitions;
#[cfg(any(test, feature = "test-gameplay"))]
mod element_execution;
mod geometry;
mod load;
mod state;
mod structural_execution;
mod support_index;

pub use analysis::{
    StructuralAnalysis, StructuralAnalysisError, StructuralAssessment, StructuralDamageEvent,
    StructuralFailureCause, StructuralStage, analyze_structure, calculate_pristine_member_capacity,
    calculate_structural_utilization_ppm,
};
pub use definitions::{
    STRUCTURAL_PARTS_PER_MILLION, StructuralLoadMode, StructuralProfileDefinition,
    StructuralProfileId, StructuralRegistry,
};
pub use geometry::{
    StructuralGeometryError, calculate_prismatic_material_mass_ceiling,
    calculate_prismatic_volume_ceiling,
};
pub use load::{calculate_aggregate_weight_force_ceiling, calculate_weight_force_ceiling};
pub use state::{
    StructuralElementGeometry, StructuralElementId, StructuralElementRecord, StructuralLifecycle,
    StructuralLoadKind, StructureState, StructureValidationError,
};
pub(crate) use structural_execution::StructuralMutationOutcome;
pub use structural_execution::{StructuralCommitError, StructuralMutationError};

#[cfg(test)]
pub(crate) use element_execution::AddStructuralElementError;
#[cfg(any(test, feature = "test-gameplay"))]
pub(crate) use element_execution::add_structural_element;
pub(crate) use structural_execution::ValidatedStructuralMutation;
#[cfg(any(test, feature = "test-gameplay"))]
pub(crate) use structural_execution::validate_activate_structural_element;

#[cfg(test)]
pub(crate) use structural_execution::{
    validate_link_support, validate_remove_structural_element, validate_set_structural_load,
};

#[cfg(feature = "test-gameplay")]
pub(crate) use construction_execution::{
    bind_structural_construction_selection, resolve_structural_material_requirement,
    validate_structural_construction,
};
pub(crate) use state::validate_loaded_structure;
pub(crate) use structural_execution::{
    ValidatedStructuralLoadBatch, validate_set_owned_structural_load,
    validate_set_owned_structural_loads,
};
pub(crate) use support_index::{
    SupportIndexValidationFault, apply_support_index_change, validate_support_index,
};

#[cfg(test)]
pub(crate) use construction_execution::materialize_structural_element_for_test;

#[cfg(test)]
pub(crate) fn make_test_structural_geometry(
    bounds: crate::spatial::VoxelBounds,
    length: crate::core::quantity::Length,
    cross_section: crate::core::quantity::Area,
) -> StructuralElementGeometry {
    match StructuralElementGeometry::new(bounds, length, cross_section) {
        Ok(geometry) => geometry,
        Err(error) => panic!("structural test geometry is invalid: {error}"),
    }
}
