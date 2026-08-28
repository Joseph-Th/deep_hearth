//! Structural support topology and derived-index validation.

use super::super::{StructuralElementId, StructuralLifecycle, StructureState};
use super::StructureValidationError;

pub(super) fn validate_support_index_coverage(
    state: &StructureState,
) -> Result<(), StructureValidationError> {
    for id in state
        .supports_by_element
        .keys()
        .chain(state.dependents_by_support.keys())
    {
        if !state.elements.contains_key(id) {
            return Err(StructureValidationError::OrphanSupportIndex { element: *id });
        }
    }
    Ok(())
}

pub(super) fn validate_support_edges(
    state: &StructureState,
) -> Result<(), StructureValidationError> {
    for (element, supports) in &state.supports_by_element {
        for support in supports {
            validate_support_edge(state, *element, *support)?;
        }
    }
    Ok(())
}

fn validate_support_edge(
    state: &StructureState,
    element: StructuralElementId,
    support: StructuralElementId,
) -> Result<(), StructureValidationError> {
    if element == support {
        return Err(StructureValidationError::SelfSupport { element });
    }
    if state.elements[&element].is_grounded() {
        return Err(StructureValidationError::GroundedElementHasSupport { element, support });
    }
    let Some(support_record) = state.elements.get(&support) else {
        return Err(StructureValidationError::UnknownSupportReference { element, support });
    };
    if !state.elements[&element]
        .bounds()
        .has_contact(support_record.bounds())
    {
        return Err(StructureValidationError::SupportOutOfContact { element, support });
    }
    if !state
        .dependents_by_support
        .get(&support)
        .is_some_and(|dependents| dependents.contains(&element))
    {
        return Err(StructureValidationError::ReverseIndexMismatch { element, support });
    }
    if state.has_path(support, element) {
        return Err(StructureValidationError::SupportCycle { element, support });
    }
    Ok(())
}

pub(super) fn validate_reverse_support_edges(
    state: &StructureState,
) -> Result<(), StructureValidationError> {
    for (support, dependents) in &state.dependents_by_support {
        for element in dependents {
            if !state
                .supports_by_element
                .get(element)
                .is_some_and(|supports| supports.contains(support))
            {
                return Err(StructureValidationError::ReverseIndexMismatch {
                    element: *element,
                    support: *support,
                });
            }
        }
    }
    Ok(())
}

pub(super) fn validate_active_supports(
    state: &StructureState,
) -> Result<(), StructureValidationError> {
    for record in state.elements.values() {
        if record.lifecycle != StructuralLifecycle::Active || record.is_grounded() {
            continue;
        }
        let has_active_support =
            state
                .supports_by_element
                .get(&record.id)
                .is_some_and(|supports| {
                    supports.iter().any(|support| {
                        state.elements.get(support).is_some_and(|candidate| {
                            candidate.lifecycle == StructuralLifecycle::Active
                        })
                    })
                });
        if !has_active_support {
            return Err(StructureValidationError::ActiveElementUnsupported { element: record.id });
        }
    }
    Ok(())
}
