//! Canonical structural construction and revision-bound mutations with synchronous damage-cascade resolution.

use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Area, Force};
use crate::core::state::AppState;
use crate::equipment::EquipmentId;
use crate::material::MaterialId;
use crate::registry::Registries;
use crate::spatial::VoxelBounds;

use super::analysis::{
    StructuralAnalysis, StructuralAnalysisError, StructuralAnalysisOverlay, StructuralDamageEvent,
    analyze_structure_components_with_overlay,
};
use super::definitions::StructuralProfileId;
use super::state::{
    StructuralElementId, StructuralElementRecord, StructuralLifecycle, StructuralLoadKind,
};

/// Failure while allocating a planned structural member and its synchronized indexes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddStructuralElementError {
    UnknownProfile { profile: StructuralProfileId },
    UnknownMaterial { material: MaterialId },
    ZeroCrossSection,
    IdExhausted,
    RevisionExhausted,
}

impl Display for AddStructuralElementError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownProfile { profile } => {
                write!(formatter, "unknown structural profile {}", profile.value())
            }
            Self::UnknownMaterial { material } => {
                write!(
                    formatter,
                    "unknown structural material {}",
                    material.value()
                )
            }
            Self::ZeroCrossSection => {
                formatter.write_str("structural member cross-sectional area must be nonzero")
            }
            Self::IdExhausted => {
                formatter.write_str("structural element identifier space is exhausted")
            }
            Self::RevisionExhausted => {
                formatter.write_str("structural state revision space is exhausted")
            }
        }
    }
}

impl Error for AddStructuralElementError {}

/// Adds an inert planned member. It cannot carry or transmit load until activated canonically.
pub fn add_structural_element(
    registries: &Registries,
    state: &mut AppState,
    profile: StructuralProfileId,
    material: MaterialId,
    bounds: VoxelBounds,
    cross_section: Area,
    grounded: bool,
) -> Result<StructuralElementId, AddStructuralElementError> {
    if registries.structural().get_profile(profile).is_none() {
        return Err(AddStructuralElementError::UnknownProfile { profile });
    }
    if registries.materials().get_material(material).is_none() {
        return Err(AddStructuralElementError::UnknownMaterial { material });
    }
    if cross_section.is_zero() {
        return Err(AddStructuralElementError::ZeroCrossSection);
    }

    let structures = state.structure_state();
    let id = StructuralElementId::new(structures.next_element_id);
    let next_element_id = structures
        .next_element_id
        .checked_add(1)
        .ok_or(AddStructuralElementError::IdExhausted)?;
    let next_revision = structures
        .revision
        .checked_add(1)
        .ok_or(AddStructuralElementError::RevisionExhausted)?;
    let record = StructuralElementRecord {
        id,
        profile,
        material,
        bounds,
        cross_section,
        grounded,
        embodied_mass: crate::core::quantity::Mass::ZERO,
        embodied_material: Vec::new(),
        loads: Default::default(),
        lifecycle: StructuralLifecycle::Planned,
        cracked: false,
        created_at: state.tick(),
    };

    let structures = state.structure_state_mut();
    let previous_record = structures.elements.insert(id, record);
    let previous_supports = structures
        .supports_by_element
        .insert(id, Default::default());
    let previous_dependents = structures
        .dependents_by_support
        .insert(id, Default::default());
    debug_assert!(
        previous_record.is_none() && previous_supports.is_none() && previous_dependents.is_none(),
        "Runtime Invariant 4 (Index Uniqueness): structural allocation replaced existing state"
    );
    structures.next_element_id = next_element_id;
    structures.revision = next_revision;
    Ok(id)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StructuralMutation {
    LinkSupport {
        element: StructuralElementId,
        support: StructuralElementId,
    },
    RemoveSupport {
        element: StructuralElementId,
        support: StructuralElementId,
    },
    RemoveElement {
        element: StructuralElementId,
    },
    Activate {
        element: StructuralElementId,
    },
    SetLoadContribution {
        element: StructuralElementId,
        kind: StructuralLoadKind,
        load: Force,
    },
}

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
    ElementOwnsMatter {
        element: StructuralElementId,
        mass: crate::core::quantity::Mass,
    },
    LoadOwnedBySubsystem {
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
            Self::ElementOwnsMatter { element, mass } => write!(
                formatter,
                "structural element {} owns {} mg of embodied matter and must be deconstructed through a conserved recovery transaction",
                element.value(),
                mass.milligrams()
            ),
            Self::LoadOwnedBySubsystem { kind } => write!(
                formatter,
                "structural {kind:?} load contribution is owned by its source subsystem and cannot be set directly"
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
            Self::UnknownElement { .. }
            | Self::UnknownSupport { .. }
            | Self::ElementFailed { .. }
            | Self::ElementSupportsEquipment { .. }
            | Self::ElementOwnsMatter { .. }
            | Self::LoadOwnedBySubsystem { .. }
            | Self::SupportFailed { .. }
            | Self::GroundedElementCannotHaveSupport { .. }
            | Self::SelfSupport { .. }
            | Self::DuplicateSupport { .. }
            | Self::MissingSupport { .. }
            | Self::SupportCycle { .. }
            | Self::ElementNotPlanned { .. }
            | Self::ActivationUnsupported { .. }
            | Self::ActivationUnmaterialized { .. }
            | Self::RevisionExhausted => None,
        }
    }
}

/// Validated structural mutation bound to one exact subsystem revision and resolved cascade.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedStructuralMutation {
    operation: StructuralMutation,
    expected_revision: u64,
    next_revision: u64,
    analysis: StructuralAnalysis,
}

impl ValidatedStructuralMutation {
    #[must_use]
    pub const fn analysis(&self) -> &StructuralAnalysis {
        &self.analysis
    }

    pub(crate) const fn expected_revision(&self) -> u64 {
        self.expected_revision
    }

    /// Commits the requested structural change and every resolved damage consequence atomically.
    pub fn commit(
        self,
        state: &mut AppState,
    ) -> Result<StructuralMutationOutcome, StructuralCommitError> {
        let structures = state.structure_state_mut();
        if structures.revision != self.expected_revision {
            return Err(StructuralCommitError::StaleRevision {
                expected: self.expected_revision,
                actual: structures.revision,
            });
        }

        validate_operation_commit_state(structures, self.operation)?;
        for event in self.analysis.damage_events() {
            let element = event.element();
            if matches!(
                self.operation,
                StructuralMutation::RemoveElement { element: removed } if removed == element
            ) {
                return Err(StructuralCommitError::StateChanged { element });
            }
            if !structures.elements.contains_key(&element) {
                return Err(StructuralCommitError::StateChanged { element });
            }
        }

        apply_operation_unchecked(structures, self.operation);
        for event in self.analysis.damage_events() {
            let element = event.element();
            let Some(record) = structures.elements.get_mut(&element) else {
                debug_assert!(
                    false,
                    "Runtime Invariant 2 (Record Reference Validity): prevalidated structural damage target disappeared"
                );
                continue;
            };
            match event {
                StructuralDamageEvent::Cracked { .. } => {
                    record.cracked = true;
                }
                StructuralDamageEvent::Failed { .. } => {
                    record.cracked = true;
                    record.lifecycle = StructuralLifecycle::Failed;
                }
            }
        }
        structures.revision = self.next_revision;
        Ok(StructuralMutationOutcome {
            analysis: self.analysis,
        })
    }
}

/// Successful structural mutation including the load projection and damage generated by that change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructuralMutationOutcome {
    analysis: StructuralAnalysis,
}

impl StructuralMutationOutcome {
    #[must_use]
    pub const fn analysis(&self) -> &StructuralAnalysis {
        &self.analysis
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

fn validate_operation_commit_state(
    structures: &super::state::StructureState,
    operation: StructuralMutation,
) -> Result<(), StructuralCommitError> {
    match operation {
        StructuralMutation::LinkSupport { element, support } => {
            let Some(supports) = structures.supports_by_element.get(&element) else {
                return Err(StructuralCommitError::StateChanged { element });
            };
            let Some(dependents) = structures.dependents_by_support.get(&support) else {
                return Err(StructuralCommitError::StateChanged { element: support });
            };
            if supports.contains(&support) || dependents.contains(&element) {
                return Err(StructuralCommitError::SupportStateChanged { element, support });
            }
        }
        StructuralMutation::RemoveSupport { element, support } => {
            let Some(supports) = structures.supports_by_element.get(&element) else {
                return Err(StructuralCommitError::StateChanged { element });
            };
            let Some(dependents) = structures.dependents_by_support.get(&support) else {
                return Err(StructuralCommitError::StateChanged { element: support });
            };
            if !supports.contains(&support) || !dependents.contains(&element) {
                return Err(StructuralCommitError::SupportStateChanged { element, support });
            }
        }
        StructuralMutation::RemoveElement { element } => {
            if !structures.elements.contains_key(&element) {
                return Err(StructuralCommitError::StateChanged { element });
            }
            let Some(supports) = structures.supports_by_element.get(&element) else {
                return Err(StructuralCommitError::StateChanged { element });
            };
            let Some(dependents) = structures.dependents_by_support.get(&element) else {
                return Err(StructuralCommitError::StateChanged { element });
            };
            for support in supports {
                let Some(reverse) = structures.dependents_by_support.get(support) else {
                    return Err(StructuralCommitError::StateChanged { element: *support });
                };
                if !reverse.contains(&element) {
                    return Err(StructuralCommitError::SupportStateChanged {
                        element,
                        support: *support,
                    });
                }
            }
            for dependent in dependents {
                let Some(forward) = structures.supports_by_element.get(dependent) else {
                    return Err(StructuralCommitError::StateChanged {
                        element: *dependent,
                    });
                };
                if !forward.contains(&element) {
                    return Err(StructuralCommitError::SupportStateChanged {
                        element: *dependent,
                        support: element,
                    });
                }
            }
        }
        StructuralMutation::Activate { element }
        | StructuralMutation::SetLoadContribution { element, .. } => {
            if !structures.elements.contains_key(&element) {
                return Err(StructuralCommitError::StateChanged { element });
            }
        }
    }
    Ok(())
}

fn apply_operation_unchecked(
    structures: &mut super::state::StructureState,
    operation: StructuralMutation,
) {
    match operation {
        StructuralMutation::LinkSupport { element, support } => {
            let Some(supports) = structures.supports_by_element.get_mut(&element) else {
                debug_assert!(false, "prevalidated structural support source disappeared");
                return;
            };
            let inserted_support = supports.insert(support);
            let Some(dependents) = structures.dependents_by_support.get_mut(&support) else {
                debug_assert!(false, "prevalidated structural support target disappeared");
                return;
            };
            let inserted_dependent = dependents.insert(element);
            debug_assert!(inserted_support && inserted_dependent);
        }
        StructuralMutation::RemoveSupport { element, support } => {
            let Some(supports) = structures.supports_by_element.get_mut(&element) else {
                debug_assert!(false, "prevalidated structural support source disappeared");
                return;
            };
            let removed_support = supports.remove(&support);
            let Some(dependents) = structures.dependents_by_support.get_mut(&support) else {
                debug_assert!(false, "prevalidated structural support target disappeared");
                return;
            };
            let removed_dependent = dependents.remove(&element);
            debug_assert!(removed_support && removed_dependent);
        }
        StructuralMutation::RemoveElement { element } => {
            let Some(supports) = structures.supports_by_element.get(&element).cloned() else {
                debug_assert!(
                    false,
                    "prevalidated removed structural element lost support index"
                );
                return;
            };
            let Some(dependents) = structures.dependents_by_support.get(&element).cloned() else {
                debug_assert!(
                    false,
                    "prevalidated removed structural element lost dependent index"
                );
                return;
            };
            for support in supports {
                if let Some(reverse) = structures.dependents_by_support.get_mut(&support) {
                    let removed = reverse.remove(&element);
                    debug_assert!(removed);
                } else {
                    debug_assert!(false, "prevalidated structural reverse index disappeared");
                }
            }
            for dependent in dependents {
                if let Some(forward) = structures.supports_by_element.get_mut(&dependent) {
                    let removed = forward.remove(&element);
                    debug_assert!(removed);
                } else {
                    debug_assert!(false, "prevalidated structural forward index disappeared");
                }
            }
            let removed_supports = structures.supports_by_element.remove(&element);
            let removed_dependents = structures.dependents_by_support.remove(&element);
            let removed_record = structures.elements.remove(&element);
            debug_assert!(
                removed_supports.is_some()
                    && removed_dependents.is_some()
                    && removed_record.is_some()
            );
        }
        StructuralMutation::Activate { element } => {
            if let Some(record) = structures.elements.get_mut(&element) {
                record.lifecycle = StructuralLifecycle::Active;
            } else {
                debug_assert!(
                    false,
                    "prevalidated structural activation target disappeared"
                );
            }
        }
        StructuralMutation::SetLoadContribution {
            element,
            kind,
            load,
        } => {
            if let Some(record) = structures.elements.get_mut(&element) {
                if load.is_zero() {
                    record.loads.remove(&kind);
                } else {
                    record.loads.insert(kind, load);
                }
            } else {
                debug_assert!(false, "prevalidated structural load target disappeared");
            }
        }
    }
}

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
    validate_operation_commit_state(state.structure_state(), operation).map_err(
        |error| match error {
            StructuralCommitError::StaleRevision { .. } => {
                StructuralMutationError::RevisionExhausted
            }
            StructuralCommitError::StateChanged { element } => {
                StructuralMutationError::UnknownElement { element }
            }
            StructuralCommitError::SupportStateChanged { element, support } => {
                StructuralMutationError::MissingSupport { element, support }
            }
        },
    )?;
    let overlay = match operation {
        StructuralMutation::LinkSupport { element, support } => {
            StructuralAnalysisOverlay::link_support(element, support)
        }
        StructuralMutation::RemoveSupport { element, support } => {
            StructuralAnalysisOverlay::remove_support(element, support)
        }
        StructuralMutation::RemoveElement { element } => {
            StructuralAnalysisOverlay::remove_element(element)
        }
        StructuralMutation::Activate { element } => StructuralAnalysisOverlay::activate(element),
        StructuralMutation::SetLoadContribution {
            element,
            kind,
            load,
        } => StructuralAnalysisOverlay::set_load(element, kind, load),
    };
    let seeds = match operation {
        StructuralMutation::LinkSupport { element, support }
        | StructuralMutation::RemoveSupport { element, support } => {
            BTreeSet::from([element, support])
        }
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
        StructuralMutation::Activate { element }
        | StructuralMutation::SetLoadContribution { element, .. } => BTreeSet::from([element]),
    };
    let analysis = analyze_structure_components_with_overlay(
        registries.structural(),
        registries.materials(),
        state.structure_state(),
        overlay,
        &seeds,
    )
    .map_err(StructuralMutationError::Analysis)?;
    Ok(ValidatedStructuralMutation {
        operation,
        expected_revision,
        next_revision,
        analysis,
    })
}

/// Validates adding a deterministic load path from one member to another.
pub fn validate_link_support(
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
    if state
        .structures()
        .supports(element)
        .is_some_and(|supports| supports.into_iter().any(|candidate| candidate == support))
    {
        return Err(StructuralMutationError::DuplicateSupport { element, support });
    }
    if state.structure_state().has_path(support, element) {
        return Err(StructuralMutationError::SupportCycle { element, support });
    }
    build_plan(
        registries,
        state,
        StructuralMutation::LinkSupport { element, support },
    )
}

/// Validates removal of one load path; unsupported dependents fail in the same eventual commit.
pub fn validate_remove_support(
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
pub fn validate_remove_structural_element(
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

/// Internal removal plan used only by a cross-owner deconstruction transaction that transfers all
/// embodied matter before the operation is exposed as successful.
pub(crate) fn validate_remove_structural_element_with_recovery(
    registries: &Registries,
    state: &AppState,
    element: StructuralElementId,
) -> Result<ValidatedStructuralMutation, StructuralMutationError> {
    if state.structures().get_element(element).is_none() {
        return Err(StructuralMutationError::UnknownElement { element });
    }
    if let Some(equipment) = state.equipment().supported_equipment(element).next() {
        return Err(StructuralMutationError::ElementSupportsEquipment { element, equipment });
    }
    build_plan(
        registries,
        state,
        StructuralMutation::RemoveElement { element },
    )
}

/// Validates transition from construction planning into the active load-bearing graph.
pub fn validate_activate_structural_element(
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
pub fn validate_set_structural_load(
    registries: &Registries,
    state: &AppState,
    element: StructuralElementId,
    kind: StructuralLoadKind,
    load: Force,
) -> Result<ValidatedStructuralMutation, StructuralMutationError> {
    validate_common_element(state, element)?;
    if matches!(
        kind,
        StructuralLoadKind::Equipment | StructuralLoadKind::SelfWeight
    ) {
        return Err(StructuralMutationError::LoadOwnedBySubsystem { kind });
    }
    validate_set_owned_structural_load(registries, state, element, kind, load)
}

/// Validates a load contribution written by the subsystem that owns that physical source.
///
/// Owned integrations may need to clear their contribution from failed debris, which is why this
/// internal boundary requires only record existence while the public arbitrary-load API rejects
/// failed elements.
pub(crate) fn validate_set_owned_structural_load(
    registries: &Registries,
    state: &AppState,
    element: StructuralElementId,
    kind: StructuralLoadKind,
    load: Force,
) -> Result<ValidatedStructuralMutation, StructuralMutationError> {
    if state.structures().get_element(element).is_none() {
        return Err(StructuralMutationError::UnknownElement { element });
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{
        FORM_LOG, MATERIAL_WOOD, STRUCTURAL_PROFILE_AXIAL_COMPRESSION, build_registries,
    };
    use crate::core::quantity::Mass;
    use crate::core::time::WorldSeed;
    use crate::inventory::add_stockpile;
    use crate::spatial::VoxelCoord;
    use crate::structural::{
        StructuralFailureCause, StructuralStage, ValidatedStructuralDeconstruction,
        make_test_deconstruction_resolution, materialize_structural_element_for_test,
        validate_structural_deconstruction,
    };

    const MEMBER_AREA: Area = Area::from_square_millimeters(1_000);
    const WOOD_COMPRESSION_CAPACITY_MN: u128 = 40_000_000;

    fn make_test_bounds(x: i64, y: i64) -> VoxelBounds {
        match VoxelBounds::new(VoxelCoord::new(x, y, 0), VoxelCoord::new(x + 1, y + 1, 1)) {
            Ok(bounds) => bounds,
            Err(error) => panic!("structural bounds fixture failed: {error}"),
        }
    }

    fn validate_test_deconstruction(
        registries: &Registries,
        state: &mut AppState,
        element: StructuralElementId,
    ) -> ValidatedStructuralDeconstruction {
        let mass = match state.structures().get_element(element) {
            Some(record) => record.embodied_mass(),
            None => panic!("deconstruction fixture references missing structural element"),
        };
        let destination = match add_stockpile(state, mass) {
            Ok(destination) => destination,
            Err(error) => panic!("deconstruction fixture stockpile failed: {error}"),
        };
        match validate_structural_deconstruction(
            registries,
            state,
            make_test_deconstruction_resolution(element, destination),
        ) {
            Ok(token) => token,
            Err(error) => panic!("deconstruction fixture validation failed: {error}"),
        }
    }

    fn commit_test_deconstruction(token: ValidatedStructuralDeconstruction, state: &mut AppState) {
        if let Err(error) = token.commit(state) {
            panic!("deconstruction fixture commit failed: {error}");
        }
    }

    fn make_test_element(
        registries: &Registries,
        state: &mut AppState,
        x: i64,
        y: i64,
        grounded: bool,
    ) -> StructuralElementId {
        let element = match add_structural_element(
            registries,
            state,
            STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
            MATERIAL_WOOD,
            make_test_bounds(x, y),
            MEMBER_AREA,
            grounded,
        ) {
            Ok(element) => element,
            Err(error) => panic!("structural element fixture failed: {error}"),
        };
        materialize_structural_element_for_test(
            registries,
            state,
            element,
            FORM_LOG,
            Mass::from_milligrams(1),
        );
        element
    }

    fn commit_test_mutation(
        token: ValidatedStructuralMutation,
        state: &mut AppState,
    ) -> StructuralMutationOutcome {
        match token.commit(state) {
            Ok(outcome) => outcome,
            Err(error) => panic!("structural mutation fixture commit failed: {error}"),
        }
    }

    fn activate_test_element(
        registries: &Registries,
        state: &mut AppState,
        element: StructuralElementId,
    ) {
        let token = match validate_activate_structural_element(registries, state, element) {
            Ok(token) => token,
            Err(error) => panic!("structural activation fixture failed: {error}"),
        };
        commit_test_mutation(token, state);
    }

    fn link_test_support(
        registries: &Registries,
        state: &mut AppState,
        element: StructuralElementId,
        support: StructuralElementId,
    ) {
        let token = match validate_link_support(registries, state, element, support) {
            Ok(token) => token,
            Err(error) => panic!("structural support fixture failed: {error}"),
        };
        commit_test_mutation(token, state);
    }

    fn find_assessment(
        outcome: &StructuralMutationOutcome,
        element: StructuralElementId,
    ) -> super::super::analysis::StructuralAssessment {
        match outcome
            .analysis()
            .assessments()
            .iter()
            .copied()
            .find(|assessment| assessment.element() == element)
        {
            Some(assessment) => assessment,
            None => panic!(
                "structural assessment fixture missing element {}",
                element.value()
            ),
        }
    }

    #[test]
    fn load_distribution_preserves_force_and_uses_stable_support_order() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x5100_0001));
        let left = make_test_element(&registries, &mut state, 0, 0, true);
        let right = make_test_element(&registries, &mut state, 2, 0, true);
        let deck = make_test_element(&registries, &mut state, 1, 1, false);
        activate_test_element(&registries, &mut state, left);
        activate_test_element(&registries, &mut state, right);
        link_test_support(&registries, &mut state, deck, left);
        link_test_support(&registries, &mut state, deck, right);
        activate_test_element(&registries, &mut state, deck);

        let token = match validate_set_structural_load(
            &registries,
            &state,
            deck,
            StructuralLoadKind::Occupancy,
            Force::from_millinewtons(30_000_001),
        ) {
            Ok(token) => token,
            Err(error) => panic!("structural load validation failed: {error}"),
        };
        let outcome = commit_test_mutation(token, &mut state);
        let deck_assessment = find_assessment(&outcome, deck);
        let left_assessment = find_assessment(&outcome, left);
        let right_assessment = find_assessment(&outcome, right);

        assert_eq!(
            deck_assessment.pristine_capacity(),
            Force::from_millinewtons(WOOD_COMPRESSION_CAPACITY_MN)
        );
        assert_eq!(deck_assessment.stage(), StructuralStage::Strained);
        assert_eq!(deck_assessment.carried_load().millinewtons(), 30_000_002);
        assert_eq!(left_assessment.carried_load().millinewtons(), 15_000_002);
        assert_eq!(right_assessment.carried_load().millinewtons(), 15_000_002);
        assert_eq!(
            left_assessment.carried_load().millinewtons() - 1
                + right_assessment.carried_load().millinewtons()
                - 1,
            deck_assessment.carried_load().millinewtons()
        );
        assert_eq!(left_assessment.stage(), StructuralStage::Stable);
        assert_eq!(right_assessment.stage(), StructuralStage::Stable);
    }

    #[test]
    fn independent_load_sources_accumulate_without_overwriting_and_zero_removes_source() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x5100_0008));
        let column = make_test_element(&registries, &mut state, 0, 0, true);
        activate_test_element(&registries, &mut state, column);

        let permanent = match validate_set_structural_load(
            &registries,
            &state,
            column,
            StructuralLoadKind::Permanent,
            Force::from_millinewtons(10_000_000),
        ) {
            Ok(token) => token,
            Err(error) => panic!("permanent load validation failed: {error}"),
        };
        commit_test_mutation(permanent, &mut state);

        let snow = match validate_set_structural_load(
            &registries,
            &state,
            column,
            StructuralLoadKind::Snow,
            Force::from_millinewtons(20_000_000),
        ) {
            Ok(token) => token,
            Err(error) => panic!("snow load validation failed: {error}"),
        };
        let combined = commit_test_mutation(snow, &mut state);
        assert_eq!(
            find_assessment(&combined, column)
                .carried_load()
                .millinewtons(),
            30_000_001
        );
        assert_eq!(
            find_assessment(&combined, column).stage(),
            StructuralStage::Strained
        );
        let record = match state.structures().get_element(column) {
            Some(record) => record,
            None => panic!("column disappeared after independent load updates"),
        };
        assert_eq!(
            record.load(StructuralLoadKind::Permanent),
            Force::from_millinewtons(10_000_000)
        );
        assert_eq!(
            record.load(StructuralLoadKind::Snow),
            Force::from_millinewtons(20_000_000)
        );

        let clear_snow = match validate_set_structural_load(
            &registries,
            &state,
            column,
            StructuralLoadKind::Snow,
            Force::ZERO,
        ) {
            Ok(token) => token,
            Err(error) => panic!("snow load removal validation failed: {error}"),
        };
        let cleared = commit_test_mutation(clear_snow, &mut state);
        assert_eq!(
            find_assessment(&cleared, column)
                .carried_load()
                .millinewtons(),
            10_000_001
        );
        let record = match state.structures().get_element(column) {
            Some(record) => record,
            None => panic!("column disappeared after clearing snow load"),
        };
        assert_eq!(record.load(StructuralLoadKind::Snow), Force::ZERO);
        assert_eq!(record.loads().count(), 2);
    }

    #[test]
    fn self_weight_load_channel_rejects_generic_writes() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x5100_0013));
        let member = make_test_element(&registries, &mut state, 0, 0, true);
        let before = state.clone();

        assert_eq!(
            validate_set_structural_load(
                &registries,
                &state,
                member,
                StructuralLoadKind::SelfWeight,
                Force::from_millinewtons(999),
            ),
            Err(StructuralMutationError::LoadOwnedBySubsystem {
                kind: StructuralLoadKind::SelfWeight,
            })
        );
        assert_eq!(state, before);
    }

    #[test]
    fn mutation_analysis_is_scoped_to_connected_structure_components() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x5100_0012));
        let support = make_test_element(&registries, &mut state, 0, 0, true);
        let deck = make_test_element(&registries, &mut state, 0, 1, false);
        activate_test_element(&registries, &mut state, support);
        link_test_support(&registries, &mut state, deck, support);
        activate_test_element(&registries, &mut state, deck);

        for index in 0_i64..256 {
            let unrelated = make_test_element(&registries, &mut state, 10 + index, 0, true);
            activate_test_element(&registries, &mut state, unrelated);
        }

        let token = match validate_set_structural_load(
            &registries,
            &state,
            deck,
            StructuralLoadKind::Snow,
            Force::from_millinewtons(20_000_000),
        ) {
            Ok(token) => token,
            Err(error) => panic!("component-scoped load validation failed: {error}"),
        };
        let assessed: Vec<_> = token
            .analysis()
            .assessments()
            .iter()
            .map(|assessment| assessment.element())
            .collect();

        assert_eq!(assessed, vec![support, deck]);
        assert!(token.analysis().damage_events().is_empty());
        commit_test_mutation(token, &mut state);
        assert_eq!(state.structures().elements().count(), 258);
    }

    #[test]
    fn planned_load_contribution_overflow_is_rejected_without_mutation() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x5100_0009));
        let member = make_test_element(&registries, &mut state, 0, 0, true);
        let maximum = match validate_set_structural_load(
            &registries,
            &state,
            member,
            StructuralLoadKind::Permanent,
            Force::from_millinewtons(u128::MAX - 1),
        ) {
            Ok(token) => token,
            Err(error) => panic!("maximum planned load validation failed: {error}"),
        };
        commit_test_mutation(maximum, &mut state);
        let before = state.clone();

        assert_eq!(
            validate_set_structural_load(
                &registries,
                &state,
                member,
                StructuralLoadKind::Snow,
                Force::from_millinewtons(1),
            ),
            Err(StructuralMutationError::Analysis(
                StructuralAnalysisError::AppliedLoadOverflow { element: member }
            ))
        );
        assert_eq!(state, before);
    }

    #[test]
    fn crack_damage_persists_after_unloading_and_reduces_later_failure_capacity() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x5100_0002));
        let column = make_test_element(&registries, &mut state, 0, 0, true);
        activate_test_element(&registries, &mut state, column);

        let crack_token = match validate_set_structural_load(
            &registries,
            &state,
            column,
            StructuralLoadKind::Permanent,
            Force::from_millinewtons(35_000_000),
        ) {
            Ok(token) => token,
            Err(error) => panic!("cracking load validation failed: {error}"),
        };
        assert_eq!(crack_token.analysis().damage_events().len(), 1);
        assert!(matches!(
            crack_token.analysis().damage_events()[0],
            StructuralDamageEvent::Cracked { element, .. } if element == column
        ));
        let cracked_outcome = commit_test_mutation(crack_token, &mut state);
        assert_eq!(
            find_assessment(&cracked_outcome, column).stage(),
            StructuralStage::Cracking
        );
        assert!(
            state
                .structures()
                .get_element(column)
                .is_some_and(|record| record.is_cracked())
        );

        let unload_token = match validate_set_structural_load(
            &registries,
            &state,
            column,
            StructuralLoadKind::Permanent,
            Force::from_millinewtons(10_000_000),
        ) {
            Ok(token) => token,
            Err(error) => panic!("unload validation failed: {error}"),
        };
        let unload_outcome = commit_test_mutation(unload_token, &mut state);
        assert!(unload_outcome.analysis().damage_events().is_empty());
        assert_eq!(
            find_assessment(&unload_outcome, column).stage(),
            StructuralStage::Cracking
        );

        let failure_token = match validate_set_structural_load(
            &registries,
            &state,
            column,
            StructuralLoadKind::Permanent,
            Force::from_millinewtons(37_000_000),
        ) {
            Ok(token) => token,
            Err(error) => panic!("post-crack overload validation failed: {error}"),
        };
        assert!(matches!(
            failure_token.analysis().damage_events(),
            [StructuralDamageEvent::Failed {
                element,
                cause: StructuralFailureCause::Overloaded {
                    effective_capacity,
                    ..
                }
            }] if *element == column && effective_capacity.millinewtons() == 36_000_000
        ));
        let failure_outcome = commit_test_mutation(failure_token, &mut state);
        assert_eq!(
            find_assessment(&failure_outcome, column).stage(),
            StructuralStage::Failed
        );
        assert_eq!(
            state
                .structures()
                .get_element(column)
                .map(|record| record.lifecycle()),
            Some(StructuralLifecycle::Failed)
        );
    }

    #[test]
    fn removing_one_load_path_cascades_failure_through_dependents_atomically() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x5100_0003));
        let foundation = make_test_element(&registries, &mut state, 0, 0, true);
        let middle = make_test_element(&registries, &mut state, 0, 1, false);
        let top = make_test_element(&registries, &mut state, 0, 2, false);
        activate_test_element(&registries, &mut state, foundation);
        link_test_support(&registries, &mut state, middle, foundation);
        activate_test_element(&registries, &mut state, middle);
        link_test_support(&registries, &mut state, top, middle);
        activate_test_element(&registries, &mut state, top);

        let token = match validate_remove_support(&registries, &state, middle, foundation) {
            Ok(token) => token,
            Err(error) => panic!("support removal validation failed: {error}"),
        };
        assert_eq!(token.analysis().damage_events().len(), 2);
        assert!(
            token
                .analysis()
                .damage_events()
                .iter()
                .all(|event| matches!(
                    event,
                    StructuralDamageEvent::Failed {
                        cause: StructuralFailureCause::Unsupported,
                        ..
                    }
                ))
        );
        let before_revision = state.structures().revision();
        commit_test_mutation(token, &mut state);

        assert_eq!(state.structures().revision(), before_revision + 1);
        assert_eq!(
            state
                .structures()
                .get_element(foundation)
                .map(|record| record.lifecycle()),
            Some(StructuralLifecycle::Active)
        );
        assert_eq!(
            state
                .structures()
                .get_element(middle)
                .map(|record| record.lifecycle()),
            Some(StructuralLifecycle::Failed)
        );
        assert_eq!(
            state
                .structures()
                .get_element(top)
                .map(|record| record.lifecycle()),
            Some(StructuralLifecycle::Failed)
        );
    }

    #[test]
    fn removing_member_redistributes_to_surviving_support_and_cleans_indexes() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x5100_0010));
        let left = make_test_element(&registries, &mut state, 0, 0, true);
        let right = make_test_element(&registries, &mut state, 2, 0, true);
        let deck = make_test_element(&registries, &mut state, 1, 1, false);
        activate_test_element(&registries, &mut state, left);
        activate_test_element(&registries, &mut state, right);
        link_test_support(&registries, &mut state, deck, left);
        link_test_support(&registries, &mut state, deck, right);
        activate_test_element(&registries, &mut state, deck);
        let load = match validate_set_structural_load(
            &registries,
            &state,
            deck,
            StructuralLoadKind::Occupancy,
            Force::from_millinewtons(30_000_000),
        ) {
            Ok(token) => token,
            Err(error) => panic!("member removal load fixture failed: {error}"),
        };
        commit_test_mutation(load, &mut state);

        let removal = validate_test_deconstruction(&registries, &mut state, left);
        assert!(removal.structural_analysis().damage_events().is_empty());
        let right_assessment = match removal
            .structural_analysis()
            .assessments()
            .iter()
            .copied()
            .find(|assessment| assessment.element() == right)
        {
            Some(assessment) => assessment,
            None => panic!("surviving support assessment disappeared during removal planning"),
        };
        assert_eq!(right_assessment.carried_load().millinewtons(), 30_000_002);
        assert_eq!(right_assessment.stage(), StructuralStage::Strained);
        commit_test_deconstruction(removal, &mut state);

        assert!(state.structures().get_element(left).is_none());
        assert!(state.structures().supports(left).is_none());
        assert!(state.structures().dependents(left).is_none());
        let deck_supports: Vec<_> = match state.structures().supports(deck) {
            Some(supports) => supports.collect(),
            None => panic!("deck support index disappeared after member removal"),
        };
        assert_eq!(deck_supports, vec![right]);
        let right_dependents: Vec<_> = match state.structures().dependents(right) {
            Some(dependents) => dependents.collect(),
            None => panic!("surviving support reverse index disappeared"),
        };
        assert_eq!(right_dependents, vec![deck]);
        assert_eq!(
            crate::core::state::validate_loaded_state(&registries, &state),
            Ok(())
        );
    }

    #[test]
    fn failed_debris_can_be_removed_and_rebuilt_without_reusing_identity() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x5100_0011));
        let foundation = make_test_element(&registries, &mut state, 0, 0, true);
        let middle = make_test_element(&registries, &mut state, 0, 1, false);
        let top = make_test_element(&registries, &mut state, 0, 2, false);
        activate_test_element(&registries, &mut state, foundation);
        link_test_support(&registries, &mut state, middle, foundation);
        activate_test_element(&registries, &mut state, middle);
        link_test_support(&registries, &mut state, top, middle);
        activate_test_element(&registries, &mut state, top);

        let remove_foundation = validate_test_deconstruction(&registries, &mut state, foundation);
        assert_eq!(
            remove_foundation
                .structural_analysis()
                .damage_events()
                .len(),
            2
        );
        commit_test_deconstruction(remove_foundation, &mut state);
        assert_eq!(
            state
                .structures()
                .get_element(middle)
                .map(|record| record.lifecycle()),
            Some(StructuralLifecycle::Failed)
        );
        assert_eq!(
            state
                .structures()
                .get_element(top)
                .map(|record| record.lifecycle()),
            Some(StructuralLifecycle::Failed)
        );

        for debris in [top, middle] {
            let token = validate_test_deconstruction(&registries, &mut state, debris);
            commit_test_deconstruction(token, &mut state);
            assert!(state.structures().get_element(debris).is_none());
        }
        assert_eq!(state.structures().elements().count(), 0);

        let replacement_foundation = make_test_element(&registries, &mut state, 0, 0, true);
        let replacement_upper = make_test_element(&registries, &mut state, 0, 1, false);
        assert!(replacement_foundation > top);
        assert!(replacement_upper > replacement_foundation);
        activate_test_element(&registries, &mut state, replacement_foundation);
        link_test_support(
            &registries,
            &mut state,
            replacement_upper,
            replacement_foundation,
        );
        activate_test_element(&registries, &mut state, replacement_upper);

        assert_eq!(
            crate::core::state::validate_loaded_state(&registries, &state),
            Ok(())
        );
        assert_eq!(
            state
                .structures()
                .get_element(replacement_upper)
                .map(|record| record.lifecycle()),
            Some(StructuralLifecycle::Active)
        );
    }

    #[test]
    fn support_cycle_is_rejected_before_mutation() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x5100_0004));
        let first = make_test_element(&registries, &mut state, 0, 0, false);
        let second = make_test_element(&registries, &mut state, 1, 0, false);
        link_test_support(&registries, &mut state, first, second);
        let before = state.clone();

        assert_eq!(
            validate_link_support(&registries, &state, second, first),
            Err(StructuralMutationError::SupportCycle {
                element: second,
                support: first,
            })
        );
        assert_eq!(state, before);
    }

    #[test]
    fn unsupported_planned_member_cannot_activate() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x5100_0005));
        let member = make_test_element(&registries, &mut state, 0, 0, false);
        let before = state.clone();

        assert_eq!(
            validate_activate_structural_element(&registries, &state, member),
            Err(StructuralMutationError::ActivationUnsupported { element: member })
        );
        assert_eq!(state, before);
    }

    #[test]
    fn stale_structural_token_cannot_overwrite_later_structural_mutation() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x5100_0006));
        let member = make_test_element(&registries, &mut state, 0, 0, true);
        activate_test_element(&registries, &mut state, member);
        let expected_revision = state.structures().revision();
        let stale = match validate_set_structural_load(
            &registries,
            &state,
            member,
            StructuralLoadKind::Permanent,
            Force::from_millinewtons(1_000_000),
        ) {
            Ok(token) => token,
            Err(error) => panic!("stale token fixture failed: {error}"),
        };
        make_test_element(&registries, &mut state, 4, 0, true);
        let before_commit = state.clone();

        assert_eq!(
            stale.commit(&mut state),
            Err(StructuralCommitError::StaleRevision {
                expected: expected_revision,
                actual: expected_revision + 2,
            })
        );
        assert_eq!(state, before_commit);
    }

    #[test]
    fn long_support_chain_collapse_is_complete_and_deterministically_ordered() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x5100_0007));
        let foundation = make_test_element(&registries, &mut state, 0, 0, true);
        activate_test_element(&registries, &mut state, foundation);
        let mut support = foundation;
        let mut chain = Vec::new();

        for index in 0_i64..128 {
            let element = make_test_element(&registries, &mut state, 0, index + 1, false);
            link_test_support(&registries, &mut state, element, support);
            activate_test_element(&registries, &mut state, element);
            chain.push(element);
            support = element;
        }

        let first = chain[0];
        let token = match validate_remove_support(&registries, &state, first, foundation) {
            Ok(token) => token,
            Err(error) => panic!("long-chain collapse validation failed: {error}"),
        };
        assert_eq!(token.analysis().damage_events().len(), chain.len());
        let event_ids: Vec<_> = token
            .analysis()
            .damage_events()
            .iter()
            .map(|event| event.element())
            .collect();
        assert_eq!(event_ids, chain);
        commit_test_mutation(token, &mut state);

        assert!(chain.iter().all(|element| {
            state
                .structures()
                .get_element(*element)
                .is_some_and(|record| {
                    record.lifecycle() == StructuralLifecycle::Failed && record.is_cracked()
                })
        }));
        assert_eq!(
            state
                .structures()
                .get_element(foundation)
                .map(|record| record.lifecycle()),
            Some(StructuralLifecycle::Active)
        );
    }
}
