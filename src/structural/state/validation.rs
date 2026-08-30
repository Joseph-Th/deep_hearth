//! Validates persisted structural members, embodiment, support topology, loads, and lifecycle.

use crate::core::quantity::Acceleration;
use crate::core::time::SimulationTick;
use crate::material::MaterialRegistry;

use super::super::definitions::StructuralRegistry;
use super::StructureState;

mod element;
mod error;
mod topology;

use element::validate_structural_element;
pub use error::StructureValidationError;
use topology::{
    validate_active_supports, validate_reverse_support_edges, validate_support_edges,
    validate_support_index_coverage,
};

pub(crate) fn validate_loaded_structure(
    profiles: &StructuralRegistry,
    materials: &MaterialRegistry,
    state: &StructureState,
    current_tick: SimulationTick,
    gravity: Acceleration,
) -> Result<(), StructureValidationError> {
    validate_structure_cursor(state)?;
    for (id, record) in &state.elements {
        validate_structural_element(
            profiles,
            materials,
            state,
            *id,
            record,
            current_tick,
            gravity,
        )?;
    }
    validate_support_index_coverage(state)?;
    validate_support_edges(state)?;
    validate_reverse_support_edges(state)?;
    validate_active_supports(state)
}

fn validate_structure_cursor(state: &StructureState) -> Result<(), StructureValidationError> {
    if state.next_element_id == 0 {
        return Err(StructureValidationError::ZeroNextElementId);
    }
    if let Some(highest) = state.elements.keys().next_back().copied()
        && highest.value() >= state.next_element_id
    {
        return Err(StructureValidationError::NextElementIdNotAboveAllocated {
            next: state.next_element_id,
            highest,
        });
    }
    Ok(())
}
