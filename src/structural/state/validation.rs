//! Validates persisted structural members, embodiment, support topology, loads, and lifecycle.

use crate::core::quantity::Acceleration;
use crate::core::time::SimulationTick;
use crate::material::MaterialRegistry;

use super::super::definitions::StructuralRegistry;
use super::{StructuralElementRecord, StructureState};

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
    validate_embodied_footprint_overlap(state)?;
    validate_support_index_coverage(state)?;
    validate_support_edges(state)?;
    validate_reverse_support_edges(state)?;
    validate_active_supports(state)
}

/// Sweep only embodied full-footprint X intervals, avoiding an all-pairs search
/// across nonoverlapping settlement structures during trusted-load promotion.
fn validate_embodied_footprint_overlap(
    state: &StructureState,
) -> Result<(), StructureValidationError> {
    let mut full_members = state
        .elements
        .values()
        .filter(|record| {
            !record.embodied_material().is_empty()
                && record.geometry().covers_horizontal_footprint()
        })
        .collect::<Vec<_>>();
    full_members.sort_by_key(|record| (record.bounds().min().x(), record.id()));
    let mut active: Vec<&StructuralElementRecord> = Vec::new();
    for record in full_members {
        let bounds = record.bounds();
        active.retain(|member| member.bounds().max_exclusive().x() > bounds.min().x());
        if let Some(existing) = active
            .iter()
            .find(|member| bounds.has_intersection(member.bounds()))
        {
            let min = bounds.min();
            let occupied = existing.bounds().min();
            return Err(StructureValidationError::OverlappingEmbodiedMembers {
                element: record.id(),
                existing: existing.id(),
                voxel: crate::spatial::VoxelCoord::new(
                    min.x().max(occupied.x()),
                    min.y().max(occupied.y()),
                    min.z().max(occupied.z()),
                ),
            });
        }
        active.push(record);
    }
    Ok(())
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
