//! Revision-bound structural mutation tokens and atomic commit mechanics.

use std::collections::BTreeMap;

use crate::core::quantity::Force;
use crate::core::state::AppState;

use super::super::analysis::{StructuralAnalysis, StructuralDamageEvent};
use super::super::state::{StructuralElementId, StructuralLoadKind, StructureState};
use super::StructuralCommitError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg(any(test, feature = "test-gameplay"))]
pub(super) enum StructuralMutation {
    #[cfg(test)]
    LinkSupport {
        element: StructuralElementId,
        support: StructuralElementId,
    },
    #[cfg(test)]
    RemoveSupport {
        element: StructuralElementId,
        support: StructuralElementId,
    },
    #[cfg(test)]
    RemoveElement { element: StructuralElementId },
    #[cfg(any(test, feature = "test-gameplay"))]
    Activate { element: StructuralElementId },
    #[cfg(test)]
    SetLoadContribution {
        element: StructuralElementId,
        kind: StructuralLoadKind,
        load: Force,
    },
}

/// Validated structural mutation bound to one exact subsystem revision and resolved cascade.
#[must_use]
#[derive(Debug, PartialEq, Eq)]
#[cfg(any(test, feature = "test-gameplay"))]
pub(crate) struct ValidatedStructuralMutation {
    operation: StructuralMutation,
    expected_revision: u64,
    next_revision: u64,
    analysis: StructuralAnalysis,
}

#[cfg(any(test, feature = "test-gameplay"))]
impl ValidatedStructuralMutation {
    pub(super) fn new(
        operation: StructuralMutation,
        expected_revision: u64,
        next_revision: u64,
        analysis: StructuralAnalysis,
    ) -> Self {
        Self {
            operation,
            expected_revision,
            next_revision,
            analysis,
        }
    }

    #[cfg(test)]
    #[must_use]
    pub(crate) const fn analysis(&self) -> &StructuralAnalysis {
        &self.analysis
    }

    /// Commits the requested structural change and every resolved damage consequence atomically.
    pub(crate) fn commit(
        self,
        state: &mut AppState,
    ) -> Result<StructuralMutationOutcome, StructuralCommitError> {
        let structures = state.structure_state_mut();
        if structures.revision() != self.expected_revision {
            return Err(StructuralCommitError::StaleRevision {
                expected: self.expected_revision,
                actual: structures.revision(),
            });
        }

        validate_operation_commit_state(structures, self.operation)?;
        for event in self.analysis.damage_events() {
            let element = event.element();
            #[cfg(test)]
            if matches!(
                self.operation,
                StructuralMutation::RemoveElement { element: removed } if removed == element
            ) {
                return Err(StructuralCommitError::StateChanged { element });
            }
            if structures.get_element(element).is_none() {
                return Err(StructuralCommitError::StateChanged { element });
            }
        }

        apply_operation_unchecked(structures, self.operation);
        apply_damage_events(structures, self.analysis.damage_events());
        structures.apply_revision(self.next_revision);
        Ok(StructuralMutationOutcome {
            analysis: self.analysis,
        })
    }
}

/// Revision-bound batch of load contributions owned by one external subsystem.
///
/// The entire batch is analyzed and committed under one structural revision so a cross-owner
/// transaction never exposes an impossible intermediate load arrangement.
#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ValidatedStructuralLoadBatch {
    kind: StructuralLoadKind,
    loads: BTreeMap<StructuralElementId, Force>,
    expected_revision: u64,
    next_revision: u64,
    analysis: StructuralAnalysis,
}

/// Revision guard plus any actual structure-owned load mutation required by an external owner.
///
/// Physical quantities may change without crossing the structural force representation boundary.
/// Such a rounded no-op still remains bound to the structural revision so a validated cross-owner
/// transaction cannot commit after topology, lifecycle, or another load contribution changes.
#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ValidatedStructuralLoadChange {
    expected_revision: u64,
    structural: Option<ValidatedStructuralLoadBatch>,
}

impl ValidatedStructuralLoadChange {
    pub(super) const fn new(
        expected_revision: u64,
        structural: Option<ValidatedStructuralLoadBatch>,
    ) -> Self {
        Self {
            expected_revision,
            structural,
        }
    }

    #[must_use]
    pub(crate) const fn expected_revision(&self) -> u64 {
        self.expected_revision
    }

    #[must_use]
    pub(crate) fn analysis(&self) -> Option<&StructuralAnalysis> {
        self.structural
            .as_ref()
            .map(ValidatedStructuralLoadBatch::analysis)
    }

    #[cfg(any(test, feature = "test-gameplay"))]
    #[must_use]
    pub(crate) const fn revision_delta(&self) -> u64 {
        if self.structural.is_some() { 1 } else { 0 }
    }

    pub(crate) fn commit(
        self,
        state: &mut AppState,
    ) -> Result<Option<StructuralMutationOutcome>, StructuralCommitError> {
        let actual = state.structures().revision();
        if actual != self.expected_revision {
            return Err(StructuralCommitError::StaleRevision {
                expected: self.expected_revision,
                actual,
            });
        }
        match self.structural {
            Some(structural) => structural.commit(state).map(Some),
            None => Ok(None),
        }
    }
}

impl ValidatedStructuralLoadBatch {
    pub(super) fn new(
        kind: StructuralLoadKind,
        loads: BTreeMap<StructuralElementId, Force>,
        expected_revision: u64,
        next_revision: u64,
        analysis: StructuralAnalysis,
    ) -> Self {
        Self {
            kind,
            loads,
            expected_revision,
            next_revision,
            analysis,
        }
    }

    #[must_use]
    pub(crate) const fn analysis(&self) -> &StructuralAnalysis {
        &self.analysis
    }

    pub(crate) fn commit(
        self,
        state: &mut AppState,
    ) -> Result<StructuralMutationOutcome, StructuralCommitError> {
        let structures = state.structure_state_mut();
        if structures.revision() != self.expected_revision {
            return Err(StructuralCommitError::StaleRevision {
                expected: self.expected_revision,
                actual: structures.revision(),
            });
        }
        for element in self.loads.keys().copied() {
            if structures.get_element(element).is_none() {
                return Err(StructuralCommitError::StateChanged { element });
            }
        }
        for event in self.analysis.damage_events() {
            if structures.get_element(event.element()).is_none() {
                return Err(StructuralCommitError::StateChanged {
                    element: event.element(),
                });
            }
        }

        apply_owned_loads(structures, self.kind, self.loads);
        apply_damage_events(structures, self.analysis.damage_events());
        structures.apply_revision(self.next_revision);
        Ok(StructuralMutationOutcome {
            analysis: self.analysis,
        })
    }
}

/// Successful structural mutation including the load projection and damage generated by that change.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StructuralMutationOutcome {
    analysis: StructuralAnalysis,
}

impl StructuralMutationOutcome {
    #[must_use]
    pub(crate) const fn analysis(&self) -> &StructuralAnalysis {
        &self.analysis
    }
}

#[cfg(any(test, feature = "test-gameplay"))]
pub(super) fn validate_operation_commit_state(
    structures: &StructureState,
    operation: StructuralMutation,
) -> Result<(), StructuralCommitError> {
    match operation {
        #[cfg(test)]
        StructuralMutation::LinkSupport { element, support } => {
            validate_support_edge_state(structures, element, support, false)?
        }
        #[cfg(test)]
        StructuralMutation::RemoveSupport { element, support } => {
            validate_support_edge_state(structures, element, support, true)?
        }
        #[cfg(test)]
        StructuralMutation::RemoveElement { element } => {
            validate_element_removal_state(structures, element)?
        }
        #[cfg(any(test, feature = "test-gameplay"))]
        StructuralMutation::Activate { element } => validate_element_exists(structures, element)?,
        #[cfg(test)]
        StructuralMutation::SetLoadContribution { element, .. } => {
            validate_element_exists(structures, element)?
        }
    }
    Ok(())
}

#[cfg(any(test, feature = "test-gameplay"))]
fn validate_element_exists(
    structures: &StructureState,
    element: StructuralElementId,
) -> Result<(), StructuralCommitError> {
    if structures.get_element(element).is_none() {
        return Err(StructuralCommitError::StateChanged { element });
    }
    Ok(())
}

#[cfg(test)]
fn validate_support_edge_state(
    structures: &StructureState,
    element: StructuralElementId,
    support: StructuralElementId,
    expected_present: bool,
) -> Result<(), StructuralCommitError> {
    let Some(supports) = structures.support_set(element) else {
        return Err(StructuralCommitError::StateChanged { element });
    };
    let Some(dependents) = structures.dependent_set(support) else {
        return Err(StructuralCommitError::StateChanged { element: support });
    };
    if supports.contains(&support) != expected_present
        || dependents.contains(&element) != expected_present
    {
        return Err(StructuralCommitError::SupportStateChanged { element, support });
    }
    Ok(())
}

#[cfg(test)]
fn validate_element_removal_state(
    structures: &StructureState,
    element: StructuralElementId,
) -> Result<(), StructuralCommitError> {
    validate_element_exists(structures, element)?;
    let Some(supports) = structures.support_set(element) else {
        return Err(StructuralCommitError::StateChanged { element });
    };
    let Some(dependents) = structures.dependent_set(element) else {
        return Err(StructuralCommitError::StateChanged { element });
    };
    for support in supports {
        validate_support_edge_state(structures, element, *support, true)?;
    }
    for dependent in dependents {
        validate_support_edge_state(structures, *dependent, element, true)?;
    }
    Ok(())
}

#[cfg(any(test, feature = "test-gameplay"))]
fn apply_operation_unchecked(structures: &mut StructureState, operation: StructuralMutation) {
    match operation {
        #[cfg(test)]
        StructuralMutation::LinkSupport { element, support } => {
            structures.link_support(element, support);
        }
        #[cfg(test)]
        StructuralMutation::RemoveSupport { element, support } => {
            structures.unlink_support(element, support);
        }
        #[cfg(test)]
        StructuralMutation::RemoveElement { element } => {
            structures.remove_element(element);
        }
        #[cfg(any(test, feature = "test-gameplay"))]
        StructuralMutation::Activate { element } => {
            structures.activate_element(element);
        }
        #[cfg(test)]
        StructuralMutation::SetLoadContribution {
            element,
            kind,
            load,
        } => {
            structures.set_load(element, kind, load);
        }
    }
}

fn apply_owned_loads(
    structures: &mut StructureState,
    kind: StructuralLoadKind,
    loads: BTreeMap<StructuralElementId, Force>,
) {
    for (element, load) in loads {
        structures.set_load(element, kind, load);
    }
}

fn apply_damage_events(structures: &mut StructureState, events: &[StructuralDamageEvent]) {
    for event in events {
        let element = event.element();
        match event {
            StructuralDamageEvent::Cracked {
                element: _element,
                carried_load: _carried_load,
                pristine_capacity: _pristine_capacity,
            } => structures.apply_damage(element, false),
            StructuralDamageEvent::Failed {
                element: _element,
                cause: _cause,
            } => structures.apply_damage(element, true),
        }
    }
}
