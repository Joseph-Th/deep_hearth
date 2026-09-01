//! Revision-bound structural topology/load mutations with synchronous damage-cascade resolution.

use std::collections::BTreeMap;

use crate::core::quantity::Force;
use crate::core::state::AppState;
use crate::registry::Registries;

#[cfg(test)]
use super::analysis::StructuralDamageEvent;
use super::analysis::{StructuralAnalysisOverlay, analyze_structure_components_with_overlay};
#[cfg(test)]
use super::state::StructuralLifecycle;
use super::state::{StructuralElementId, StructuralLoadKind};

mod errors;
#[cfg(any(test, feature = "test-gameplay"))]
mod fixture;
mod transaction;

pub use errors::{StructuralCommitError, StructuralMutationError};

#[cfg(any(test, feature = "test-gameplay"))]
pub(crate) use fixture::validate_activate_structural_element;
#[cfg(test)]
pub(crate) use fixture::{
    validate_link_support, validate_remove_structural_element, validate_remove_support,
    validate_set_structural_load,
};
#[cfg(test)]
pub(crate) use transaction::ValidatedStructuralMutation;
pub(crate) use transaction::{
    StructuralMutationOutcome, ValidatedStructuralLoadBatch, ValidatedStructuralLoadChange,
};

/// Validates several load contributions owned by one external subsystem as one structural change.
///
/// Entries already equal to authoritative state are omitted. If every requested load already
/// matches, no structural revision is required, but the returned transaction remains bound to the
/// current structural revision.
pub(crate) fn validate_owned_structural_load_change(
    registries: &Registries,
    state: &AppState,
    kind: StructuralLoadKind,
    loads: BTreeMap<StructuralElementId, Force>,
) -> Result<ValidatedStructuralLoadChange, StructuralMutationError> {
    let expected_revision = state.structures().revision();
    let mut changed = BTreeMap::new();
    for (element, load) in loads {
        let record = state
            .structures()
            .get_element(element)
            .ok_or(StructuralMutationError::UnknownElement { element })?;
        if record.load(kind) != load {
            changed.insert(element, load);
        }
    }
    if changed.is_empty() {
        return Ok(ValidatedStructuralLoadChange::new(expected_revision, None));
    }

    let next_revision = expected_revision
        .checked_add(1)
        .ok_or(StructuralMutationError::RevisionExhausted)?;
    let overlay = StructuralAnalysisOverlay::set_loads(
        changed
            .iter()
            .map(|(element, load)| ((*element, kind), *load))
            .collect(),
    );
    let seeds = changed.keys().copied().collect();
    let analysis = analyze_structure_components_with_overlay(
        registries.structural(),
        registries.materials(),
        state.structures(),
        overlay,
        &seeds,
    )
    .map_err(StructuralMutationError::Analysis)?;

    Ok(ValidatedStructuralLoadChange::new(
        expected_revision,
        Some(ValidatedStructuralLoadBatch::new(
            kind,
            changed,
            expected_revision,
            next_revision,
            analysis,
        )),
    ))
}

#[cfg(test)]
#[path = "structural_execution_tests.rs"]
mod tests;
