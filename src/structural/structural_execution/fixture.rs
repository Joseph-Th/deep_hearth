//! Controlled structural topology mutation used by tests and gameplay capability setup.

use std::collections::BTreeSet;

#[cfg(test)]
use crate::core::quantity::Force;
use crate::core::state::AppState;
use crate::registry::Registries;

use super::super::analysis::{
    StructuralAnalysisOverlay, analyze_structure_components_with_overlay,
};
#[cfg(test)]
use super::super::state::StructuralLoadKind;
use super::super::state::{StructuralElementId, StructuralLifecycle};
use super::transaction::{
    StructuralMutation, ValidatedStructuralMutation, validate_operation_commit_state,
};
use super::{StructuralCommitError, StructuralMutationError};

fn validate_common_element(
    state: &AppState,
    element: StructuralElementId,
) -> Result<StructuralLifecycle, StructuralMutationError> {
    let Some(record) = state.structures().get_element(element) else {
        return Err(StructuralMutationError::UnknownElement { element });
    };
    if record.lifecycle() == StructuralLifecycle::Failed {
        return Err(StructuralMutationError::ElementFailed { element });
    }
    Ok(record.lifecycle())
}

fn build_plan(
    registries: &Registries,
    state: &AppState,
    operation: StructuralMutation,
) -> Result<ValidatedStructuralMutation, StructuralMutationError> {
    let expected_revision = state.structures().revision();
    let next_revision = expected_revision
        .checked_add(1)
        .ok_or(StructuralMutationError::RevisionExhausted)?;
    validate_operation_commit_state(state.structures(), operation).map_err(
        |error| match error {
            StructuralCommitError::StaleRevision {
                expected: _expected,
                actual: _actual,
            } => StructuralMutationError::RevisionExhausted,
            StructuralCommitError::StateChanged { element } => {
                StructuralMutationError::UnknownElement { element }
            }
            StructuralCommitError::SupportStateChanged { element, support } => {
                StructuralMutationError::MissingSupport { element, support }
            }
        },
    )?;
    let overlay = match operation {
        #[cfg(test)]
        StructuralMutation::LinkSupport { element, support } => {
            StructuralAnalysisOverlay::link_support(element, support)
        }
        #[cfg(test)]
        StructuralMutation::RemoveSupport { element, support } => {
            StructuralAnalysisOverlay::remove_support(element, support)
        }
        #[cfg(test)]
        StructuralMutation::RemoveElement { element } => {
            StructuralAnalysisOverlay::remove_element(element)
        }
        StructuralMutation::Activate { element } => StructuralAnalysisOverlay::activate(element),
        #[cfg(test)]
        StructuralMutation::SetLoadContribution {
            element,
            kind,
            load,
        } => StructuralAnalysisOverlay::set_load(element, kind, load),
    };
    let seeds = match operation {
        #[cfg(test)]
        StructuralMutation::LinkSupport { element, support }
        | StructuralMutation::RemoveSupport { element, support } => {
            BTreeSet::from([element, support])
        }
        #[cfg(test)]
        StructuralMutation::RemoveElement { element } => {
            let mut seeds = BTreeSet::new();
            if let Some(supports) = state.structures().supports(element) {
                seeds.extend(supports);
            }
            if let Some(dependents) = state.structures().dependents(element) {
                seeds.extend(dependents);
            }
            seeds
        }
        StructuralMutation::Activate { element } => BTreeSet::from([element]),
        #[cfg(test)]
        StructuralMutation::SetLoadContribution { element, .. } => BTreeSet::from([element]),
    };
    let analysis = analyze_structure_components_with_overlay(
        registries.structural(),
        registries.materials(),
        state.structures(),
        overlay,
        &seeds,
    )
    .map_err(StructuralMutationError::Analysis)?;
    Ok(ValidatedStructuralMutation::new(
        operation,
        expected_revision,
        next_revision,
        analysis,
    ))
}

/// Validates adding a deterministic load path from one member to another.
#[cfg(test)]
pub(crate) fn validate_link_support(
    registries: &Registries,
    state: &AppState,
    element: StructuralElementId,
    support: StructuralElementId,
) -> Result<ValidatedStructuralMutation, StructuralMutationError> {
    validate_common_element(state, element)?;
    let Some(element_record) = state.structures().get_element(element) else {
        return Err(StructuralMutationError::UnknownElement { element });
    };
    if element_record.is_grounded() {
        return Err(StructuralMutationError::GroundedElementCannotHaveSupport { element });
    }
    if element == support {
        return Err(StructuralMutationError::SelfSupport { element });
    }
    let Some(support_record) = state.structures().get_element(support) else {
        return Err(StructuralMutationError::UnknownSupport { support });
    };
    if support_record.lifecycle() == StructuralLifecycle::Failed {
        return Err(StructuralMutationError::SupportFailed { support });
    }
    if !element_record.bounds().has_contact(support_record.bounds()) {
        return Err(StructuralMutationError::SupportOutOfContact { element, support });
    }
    if state
        .structures()
        .supports(element)
        .is_some_and(|supports| supports.into_iter().any(|candidate| candidate == support))
    {
        return Err(StructuralMutationError::DuplicateSupport { element, support });
    }
    if state.structures().has_path(support, element) {
        return Err(StructuralMutationError::SupportCycle { element, support });
    }
    build_plan(
        registries,
        state,
        StructuralMutation::LinkSupport { element, support },
    )
}

/// Validates removal of one load path; unsupported dependents fail in the same eventual commit.
#[cfg(test)]
pub(crate) fn validate_remove_support(
    registries: &Registries,
    state: &AppState,
    element: StructuralElementId,
    support: StructuralElementId,
) -> Result<ValidatedStructuralMutation, StructuralMutationError> {
    validate_common_element(state, element)?;
    if state.structures().get_element(support).is_none() {
        return Err(StructuralMutationError::UnknownSupport { support });
    }
    if !state
        .structures()
        .supports(element)
        .is_some_and(|supports| supports.into_iter().any(|candidate| candidate == support))
    {
        return Err(StructuralMutationError::MissingSupport { element, support });
    }
    build_plan(
        registries,
        state,
        StructuralMutation::RemoveSupport { element, support },
    )
}

/// Validates removing one member entirely, cleaning its indexes and resolving loss of support.
///
/// Failed members use this same path so collapse remains recoverable rather than creating immortal
/// debris records with dangling topology.
#[cfg(test)]
pub(crate) fn validate_remove_structural_element(
    registries: &Registries,
    state: &AppState,
    element: StructuralElementId,
) -> Result<ValidatedStructuralMutation, StructuralMutationError> {
    let record = state
        .structures()
        .get_element(element)
        .ok_or(StructuralMutationError::UnknownElement { element })?;
    if let Some(equipment) = state.equipment().supported_equipment(element).next() {
        return Err(StructuralMutationError::ElementSupportsEquipment { element, equipment });
    }
    if let Some(stockpile) = state.inventory().supported_stockpiles(element).next() {
        return Err(StructuralMutationError::ElementSupportsStockpile { element, stockpile });
    }
    if let Some(store) = state.fluid().supported_stores(element).next() {
        return Err(StructuralMutationError::ElementSupportsFluidStore { element, store });
    }
    if !record.embodied_mass().is_zero() {
        return Err(StructuralMutationError::ElementOwnsMatter {
            element,
            mass: record.embodied_mass(),
        });
    }
    build_plan(
        registries,
        state,
        StructuralMutation::RemoveElement { element },
    )
}

/// Validates transition from construction planning into the active load-bearing graph.
pub(crate) fn validate_activate_structural_element(
    registries: &Registries,
    state: &AppState,
    element: StructuralElementId,
) -> Result<ValidatedStructuralMutation, StructuralMutationError> {
    let lifecycle = validate_common_element(state, element)?;
    if lifecycle != StructuralLifecycle::Planned {
        return Err(StructuralMutationError::ElementNotPlanned { element });
    }
    let Some(record) = state.structures().get_element(element) else {
        return Err(StructuralMutationError::UnknownElement { element });
    };
    if record.embodied_mass().is_zero() {
        return Err(StructuralMutationError::ActivationUnmaterialized { element });
    }
    if !record.is_grounded() {
        let has_active_support = state
            .structures()
            .supports(element)
            .is_some_and(|supports| {
                supports.into_iter().any(|support| {
                    state
                        .structures()
                        .get_element(support)
                        .is_some_and(|candidate| {
                            candidate.lifecycle() == StructuralLifecycle::Active
                        })
                })
            });
        if !has_active_support {
            return Err(StructuralMutationError::ActivationUnsupported { element });
        }
    }
    build_plan(registries, state, StructuralMutation::Activate { element })
}

/// Validates an explicit external load change and resolves all resulting cracks and failures.
#[cfg(test)]
pub(crate) fn validate_set_structural_load(
    registries: &Registries,
    state: &AppState,
    element: StructuralElementId,
    kind: StructuralLoadKind,
    load: Force,
) -> Result<ValidatedStructuralMutation, StructuralMutationError> {
    validate_common_element(state, element)?;
    if matches!(
        kind,
        StructuralLoadKind::Equipment
            | StructuralLoadKind::Fluid
            | StructuralLoadKind::SelfWeight
            | StructuralLoadKind::StoredMatter
    ) {
        return Err(StructuralMutationError::LoadOwnedBySubsystem { kind });
    }
    if state
        .structures()
        .get_element(element)
        .is_some_and(|record| record.load(kind) == load)
    {
        return Err(StructuralMutationError::LoadUnchanged {
            element,
            kind,
            load,
        });
    }
    build_plan(
        registries,
        state,
        StructuralMutation::SetLoadContribution {
            element,
            kind,
            load,
        },
    )
}
