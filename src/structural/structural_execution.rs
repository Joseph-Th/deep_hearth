//! Revision-bound structural topology/load mutations with synchronous damage-cascade resolution.

use std::collections::BTreeMap;
#[cfg(any(test, feature = "test-gameplay"))]
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Force;
use crate::core::state::AppState;
use crate::equipment::EquipmentId;
use crate::fluid::FluidStoreId;
use crate::inventory::StockpileId;
use crate::registry::Registries;

#[cfg(test)]
use super::analysis::StructuralDamageEvent;
use super::analysis::{
    StructuralAnalysisError, StructuralAnalysisOverlay, analyze_structure_components_with_overlay,
};
#[cfg(any(test, feature = "test-gameplay"))]
use super::state::StructuralLifecycle;
use super::state::{StructuralElementId, StructuralLoadKind};

mod transaction;

#[cfg(any(test, feature = "test-gameplay"))]
pub(crate) use transaction::ValidatedStructuralMutation;
#[cfg(any(test, feature = "test-gameplay"))]
use transaction::{StructuralMutation, validate_operation_commit_state};
pub(crate) use transaction::{
    StructuralMutationOutcome, ValidatedStructuralLoadBatch, ValidatedStructuralLoadChange,
};

/// Failure while validating a structural mutation before any authoritative state changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StructuralMutationError {
    UnknownElement {
        element: StructuralElementId,
    },
    UnknownSupport {
        support: StructuralElementId,
    },
    ElementFailed {
        element: StructuralElementId,
    },
    ElementSupportsEquipment {
        element: StructuralElementId,
        equipment: EquipmentId,
    },
    ElementSupportsStockpile {
        element: StructuralElementId,
        stockpile: StockpileId,
    },
    ElementSupportsFluidStore {
        element: StructuralElementId,
        store: FluidStoreId,
    },
    ElementOwnsMatter {
        element: StructuralElementId,
        mass: crate::core::quantity::Mass,
    },
    LoadOwnedBySubsystem {
        kind: StructuralLoadKind,
    },
    LoadUnchanged {
        element: StructuralElementId,
        kind: StructuralLoadKind,
        load: Force,
    },
    LoadTargetsRemovedElement {
        element: StructuralElementId,
        kind: StructuralLoadKind,
    },
    SupportFailed {
        support: StructuralElementId,
    },
    GroundedElementCannotHaveSupport {
        element: StructuralElementId,
    },
    SelfSupport {
        element: StructuralElementId,
    },
    SupportOutOfContact {
        element: StructuralElementId,
        support: StructuralElementId,
    },
    DuplicateSupport {
        element: StructuralElementId,
        support: StructuralElementId,
    },
    MissingSupport {
        element: StructuralElementId,
        support: StructuralElementId,
    },
    SupportCycle {
        element: StructuralElementId,
        support: StructuralElementId,
    },
    ElementNotPlanned {
        element: StructuralElementId,
    },
    ActivationUnsupported {
        element: StructuralElementId,
    },
    ActivationUnmaterialized {
        element: StructuralElementId,
    },
    RevisionExhausted,
    Analysis(StructuralAnalysisError),
}

impl Display for StructuralMutationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownElement { element } => {
                write!(formatter, "unknown structural element {}", element.value())
            }
            Self::UnknownSupport { support } => {
                write!(formatter, "unknown structural support {}", support.value())
            }
            Self::ElementFailed { element } => write!(
                formatter,
                "failed structural element {} cannot be reconfigured",
                element.value()
            ),
            Self::ElementSupportsEquipment { element, equipment } => write!(
                formatter,
                "structural element {} cannot be removed while it supports equipment {}",
                element.value(),
                equipment.value()
            ),
            Self::ElementSupportsStockpile { element, stockpile } => write!(
                formatter,
                "structural element {} cannot be removed while it supports stockpile {}",
                element.value(),
                stockpile.value()
            ),
            Self::ElementSupportsFluidStore { element, store } => write!(
                formatter,
                "structural element {} cannot be removed while it supports fluid store {}",
                element.value(),
                store.value()
            ),
            Self::ElementOwnsMatter { element, mass } => write!(
                formatter,
                "structural element {} owns {} mg of embodied matter and cannot be generically removed; demolition and recovery are not implemented",
                element.value(),
                mass.milligrams()
            ),
            Self::LoadOwnedBySubsystem { kind } => write!(
                formatter,
                "structural {kind:?} load contribution is owned by its source subsystem and cannot be set directly"
            ),
            Self::LoadUnchanged {
                element,
                kind,
                load,
            } => write!(
                formatter,
                "structural {kind:?} load on element {} is already {} mN",
                element.value(),
                load.millinewtons()
            ),
            Self::LoadTargetsRemovedElement { element, kind } => write!(
                formatter,
                "structural {kind:?} load cannot target element {} while that element is removed by the same mutation",
                element.value()
            ),
            Self::SupportFailed { support } => write!(
                formatter,
                "failed structural element {} cannot provide new support",
                support.value()
            ),
            Self::GroundedElementCannotHaveSupport { element } => write!(
                formatter,
                "ground-anchored structural element {} cannot also route load through a member support",
                element.value()
            ),
            Self::SelfSupport { element } => write!(
                formatter,
                "structural element {} cannot support itself",
                element.value()
            ),
            Self::SupportOutOfContact { element, support } => write!(
                formatter,
                "structural support edge {} -> {} cannot cross empty space; the member bounds do not touch or overlap",
                element.value(),
                support.value()
            ),
            Self::DuplicateSupport { element, support } => write!(
                formatter,
                "structural support edge {} -> {} already exists",
                element.value(),
                support.value()
            ),
            Self::MissingSupport { element, support } => write!(
                formatter,
                "structural support edge {} -> {} does not exist",
                element.value(),
                support.value()
            ),
            Self::SupportCycle { element, support } => write!(
                formatter,
                "structural support edge {} -> {} would create a cycle",
                element.value(),
                support.value()
            ),
            Self::ElementNotPlanned { element } => write!(
                formatter,
                "structural element {} is not in planned lifecycle",
                element.value()
            ),
            Self::ActivationUnsupported { element } => write!(
                formatter,
                "structural element {} cannot activate without an active support or ground anchor",
                element.value()
            ),
            Self::ActivationUnmaterialized { element } => write!(
                formatter,
                "structural element {} cannot activate before construction matter is committed",
                element.value()
            ),
            Self::RevisionExhausted => {
                formatter.write_str("structural state revision space is exhausted")
            }
            Self::Analysis(error) => write!(formatter, "structural analysis failed: {error}"),
        }
    }
}

impl Error for StructuralMutationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Analysis(error) => Some(error),
            Self::UnknownElement { element: _element }
            | Self::ElementFailed { element: _element }
            | Self::GroundedElementCannotHaveSupport { element: _element }
            | Self::SelfSupport { element: _element }
            | Self::ElementNotPlanned { element: _element }
            | Self::ActivationUnsupported { element: _element }
            | Self::ActivationUnmaterialized { element: _element } => None,
            Self::UnknownSupport { support: _support }
            | Self::SupportFailed { support: _support } => None,
            Self::ElementSupportsEquipment {
                element: _element,
                equipment: _equipment,
            } => None,
            Self::ElementSupportsStockpile {
                element: _element,
                stockpile: _stockpile,
            } => None,
            Self::ElementSupportsFluidStore {
                element: _element,
                store: _store,
            } => None,
            Self::ElementOwnsMatter {
                element: _element,
                mass: _mass,
            } => None,
            Self::LoadOwnedBySubsystem { kind: _kind } => None,
            Self::LoadUnchanged {
                element: _element,
                kind: _kind,
                load: _load,
            } => None,
            Self::LoadTargetsRemovedElement {
                element: _element,
                kind: _kind,
            } => None,
            Self::DuplicateSupport {
                element: _element,
                support: _support,
            }
            | Self::SupportOutOfContact {
                element: _element,
                support: _support,
            }
            | Self::MissingSupport {
                element: _element,
                support: _support,
            }
            | Self::SupportCycle {
                element: _element,
                support: _support,
            } => None,
            Self::RevisionExhausted => None,
        }
    }
}

/// A validated structural token can no longer commit because authoritative state changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StructuralCommitError {
    StaleRevision {
        expected: u64,
        actual: u64,
    },
    StateChanged {
        element: StructuralElementId,
    },
    SupportStateChanged {
        element: StructuralElementId,
        support: StructuralElementId,
    },
}

impl Display for StructuralCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleRevision { expected, actual } => write!(
                formatter,
                "structural mutation expected revision {expected} but current revision is {actual}"
            ),
            Self::StateChanged { element } => write!(
                formatter,
                "structural element {} changed after validation",
                element.value()
            ),
            Self::SupportStateChanged { element, support } => write!(
                formatter,
                "structural support edge {} -> {} changed after validation",
                element.value(),
                support.value()
            ),
        }
    }
}

impl Error for StructuralCommitError {}

#[cfg(any(test, feature = "test-gameplay"))]
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

#[cfg(any(test, feature = "test-gameplay"))]
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
        #[cfg(any(test, feature = "test-gameplay"))]
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
        #[cfg(any(test, feature = "test-gameplay"))]
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
#[cfg(any(test, feature = "test-gameplay"))]
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
