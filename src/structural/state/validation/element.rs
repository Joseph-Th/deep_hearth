//! Structural element-local validation: geometry, embodiment, loads, and lifecycle.

use crate::core::quantity::{Acceleration, AggregateMass, Mass};
use crate::core::time::SimulationTick;
use crate::inventory::ConsumedMaterialTrace;
use crate::material::{
    MaterialRegistry, validate_material_particle_size_state, validate_material_phase_state,
};

use super::super::super::definitions::StructuralRegistry;
use super::super::super::geometry::calculate_prismatic_material_mass_ceiling;
use super::super::super::load::calculate_aggregate_weight_force_ceiling;
use super::super::{
    StructuralElementId, StructuralElementRecord, StructuralLifecycle, StructuralLoadKind,
    StructureState,
};
use super::StructureValidationError;

pub(super) fn validate_structural_element(
    profiles: &StructuralRegistry,
    materials: &MaterialRegistry,
    state: &StructureState,
    id: StructuralElementId,
    record: &StructuralElementRecord,
    current_tick: SimulationTick,
    gravity: Acceleration,
) -> Result<(), StructureValidationError> {
    validate_structural_element_identity(id, record.id)?;
    profiles
        .get_profile(record.profile())
        .ok_or(StructureValidationError::UnknownProfile {
            element: record.id,
            profile: record.profile(),
        })?;
    let material = materials.get_material(record.material()).ok_or(
        StructureValidationError::UnknownMaterial {
            element: record.id,
            material: record.material(),
        },
    )?;
    if material.properties().structural().is_none() {
        return Err(StructureValidationError::NonStructuralMaterial {
            element: record.id,
            material: record.material(),
        });
    }
    validate_element_geometry_shape(record)?;
    let traced_mass = validate_element_embodiment(materials, record, current_tick)?;
    validate_element_mass_and_loads(materials, record, traced_mass, gravity)?;
    validate_element_lifecycle(record, current_tick)?;
    if !state.supports_by_element.contains_key(&id)
        || !state.dependents_by_support.contains_key(&id)
    {
        return Err(StructureValidationError::MissingSupportIndex { element: id });
    }
    Ok(())
}

fn validate_structural_element_identity(
    key: StructuralElementId,
    record: StructuralElementId,
) -> Result<(), StructureValidationError> {
    if key.value() == 0 || record.value() == 0 {
        return Err(StructureValidationError::ZeroElementId);
    }
    if key != record {
        return Err(StructureValidationError::ElementKeyMismatch { key, record });
    }
    Ok(())
}

fn validate_element_geometry_shape(
    record: &StructuralElementRecord,
) -> Result<(), StructureValidationError> {
    if record.cross_section().is_zero() {
        return Err(StructureValidationError::ZeroCrossSection { element: record.id });
    }
    if record.length().is_zero() {
        return Err(StructureValidationError::ZeroLength { element: record.id });
    }
    if record.lifecycle != StructuralLifecycle::Planned && record.embodied_mass.is_zero() {
        return Err(StructureValidationError::UnmaterializedLoadBearingElement {
            element: record.id,
            lifecycle: record.lifecycle,
        });
    }
    Ok(())
}

#[cfg(test)]
#[path = "element_tests.rs"]
mod tests;

fn validate_element_embodiment(
    materials: &MaterialRegistry,
    record: &StructuralElementRecord,
    current_tick: SimulationTick,
) -> Result<Mass, StructureValidationError> {
    let mut traced_mass = Mass::ZERO;
    for trace in &record.embodied_material {
        validate_element_embodied_trace(materials, record, trace, current_tick)?;
        traced_mass = traced_mass
            .checked_add(trace.mass())
            .ok_or(StructureValidationError::EmbodiedMassOverflow { element: record.id })?;
    }
    if traced_mass != record.embodied_mass {
        return Err(StructureValidationError::EmbodiedMassMismatch {
            element: record.id,
            stored: record.embodied_mass,
            traced: traced_mass,
        });
    }
    Ok(traced_mass)
}

fn validate_element_embodied_trace(
    materials: &MaterialRegistry,
    record: &StructuralElementRecord,
    trace: &ConsumedMaterialTrace,
    current_tick: SimulationTick,
) -> Result<(), StructureValidationError> {
    if trace.mass().is_zero() {
        return Err(StructureValidationError::ZeroEmbodiedTrace { element: record.id });
    }
    validate_embodied_trace_material_state(materials, record.id, trace)?;
    validate_embodied_trace_identity(materials, record, trace)?;
    validate_embodied_trace_provenance(record.id, trace, current_tick)
}

fn validate_embodied_trace_material_state(
    materials: &MaterialRegistry,
    element: StructuralElementId,
    trace: &ConsumedMaterialTrace,
) -> Result<(), StructureValidationError> {
    let commodity = trace.profile().commodity();
    if !materials.has_commodity(commodity) {
        return Err(StructureValidationError::UnknownEmbodiedCommodity { element });
    }
    let Some(form) = materials.get_form(commodity.form()) else {
        return Err(StructureValidationError::UnknownEmbodiedCommodity { element });
    };
    if !form.is_consolidated() {
        return Err(StructureValidationError::UnconsolidatedEmbodiedForm {
            element,
            form: commodity.form(),
        });
    }
    validate_material_phase_state(
        materials,
        commodity,
        trace.profile().composition(),
        trace.profile().temperature(),
    )
    .map_err(|error| StructureValidationError::InvalidEmbodiedPhaseState { element, error })?;
    validate_material_particle_size_state(
        materials,
        commodity,
        trace.profile().particle_size_distribution(),
    )
    .map_err(|error| StructureValidationError::InvalidEmbodiedParticleSizeState { element, error })
}

fn validate_embodied_trace_identity(
    materials: &MaterialRegistry,
    record: &StructuralElementRecord,
    trace: &ConsumedMaterialTrace,
) -> Result<(), StructureValidationError> {
    let commodity = trace.profile().commodity();
    if commodity.material() != record.material() {
        return Err(StructureValidationError::EmbodiedMaterialMismatch {
            element: record.id,
            expected: record.material(),
            found: commodity.material(),
        });
    }
    if trace.profile().composition().pure_material() != Some(record.material()) {
        return Err(StructureValidationError::UnsupportedEmbodiedComposition {
            element: record.id,
            material: record.material(),
        });
    }
    for component in trace.profile().composition().components() {
        if materials.get_material(component.material()).is_none() {
            return Err(
                StructureValidationError::UnknownEmbodiedCompositionMaterial {
                    element: record.id,
                    material: component.material(),
                },
            );
        }
    }
    Ok(())
}

fn validate_embodied_trace_provenance(
    element: StructuralElementId,
    trace: &ConsumedMaterialTrace,
    current_tick: SimulationTick,
) -> Result<(), StructureValidationError> {
    let provenance = trace.provenance();
    if provenance.latest_created_at() < provenance.earliest_created_at() {
        return Err(StructureValidationError::InvalidEmbodiedProvenanceRange { element });
    }
    if provenance.latest_created_at() > current_tick {
        return Err(StructureValidationError::EmbodiedProvenanceInFuture {
            element,
            latest_created_at: provenance.latest_created_at(),
            current: current_tick,
        });
    }
    Ok(())
}

fn validate_element_mass_and_loads(
    materials: &MaterialRegistry,
    record: &StructuralElementRecord,
    traced_mass: Mass,
    gravity: Acceleration,
) -> Result<(), StructureValidationError> {
    debug_assert_eq!(traced_mass, record.embodied_mass);
    if !record.embodied_mass.is_zero() {
        let required = calculate_prismatic_material_mass_ceiling(
            materials,
            record.material(),
            record.cross_section(),
            record.length(),
        )
        .map_err(|error| StructureValidationError::Geometry {
            element: record.id,
            error,
        })?;
        if record.embodied_mass != required {
            return Err(StructureValidationError::EmbodiedMassGeometryMismatch {
                element: record.id,
                stored: record.embodied_mass,
                required,
            });
        }
    }
    let expected_self_weight = calculate_aggregate_weight_force_ceiling(
        AggregateMass::from_mass(record.embodied_mass),
        gravity,
    )
    .ok_or(StructureValidationError::SelfWeightOverflow { element: record.id })?;
    let stored_self_weight = record.load(StructuralLoadKind::SelfWeight);
    if stored_self_weight != expected_self_weight {
        return Err(StructureValidationError::SelfWeightMismatch {
            element: record.id,
            stored: stored_self_weight,
            expected: expected_self_weight,
        });
    }
    if let Some((kind, _)) = record.loads.iter().find(|(_, load)| load.is_zero()) {
        return Err(StructureValidationError::ZeroLoadContribution {
            element: record.id,
            kind: *kind,
        });
    }
    Ok(())
}

fn validate_element_lifecycle(
    record: &StructuralElementRecord,
    current_tick: SimulationTick,
) -> Result<(), StructureValidationError> {
    if record.created_at > current_tick {
        return Err(StructureValidationError::CreatedInFuture {
            element: record.id,
            created_at: record.created_at,
            current: current_tick,
        });
    }
    if record.lifecycle == StructuralLifecycle::Planned && record.is_cracked {
        return Err(StructureValidationError::PlannedElementCracked { element: record.id });
    }
    if record.lifecycle == StructuralLifecycle::Failed && !record.is_cracked {
        return Err(StructureValidationError::FailedElementNotCracked { element: record.id });
    }
    Ok(())
}
