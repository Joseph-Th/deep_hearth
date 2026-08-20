//! Canonical structural construction and revision-bound mutations with synchronous damage-cascade resolution.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Force;
use crate::core::state::AppState;
use crate::equipment::EquipmentId;
use crate::fluid::FluidStoreId;
use crate::inventory::StockpileId;
use crate::material::MaterialId;
use crate::registry::Registries;

use super::analysis::{
    StructuralAnalysis, StructuralAnalysisError, StructuralAnalysisOverlay, StructuralDamageEvent,
    analyze_structure_components_with_overlay,
};
use super::definitions::StructuralProfileId;
use super::geometry::StructuralGeometryError;
use super::state::{
    StructuralElementConfiguration, StructuralElementGeometry, StructuralElementId,
    StructuralElementRecord, StructuralLifecycle, StructuralLoadKind,
};

/// Failure while allocating a planned structural member and its synchronized indexes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddStructuralElementError {
    UnknownProfile { profile: StructuralProfileId },
    UnknownMaterial { material: MaterialId },
    Geometry(StructuralGeometryError),
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
            Self::Geometry(error) => write!(formatter, "invalid structural geometry: {error}"),
            Self::IdExhausted => {
                formatter.write_str("structural element identifier space is exhausted")
            }
            Self::RevisionExhausted => {
                formatter.write_str("structural state revision space is exhausted")
            }
        }
    }
}

impl Error for AddStructuralElementError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Geometry(error) => Some(error),
            Self::UnknownProfile { profile: _profile } => None,
            Self::UnknownMaterial {
                material: _material,
            } => None,
            Self::IdExhausted | Self::RevisionExhausted => None,
        }
    }
}

/// Adds an inert planned member. It cannot carry or transmit load until activated canonically.
pub fn add_structural_element(
    registries: &Registries,
    state: &mut AppState,
    profile: StructuralProfileId,
    material: MaterialId,
    geometry: StructuralElementGeometry,
    is_grounded: bool,
) -> Result<StructuralElementId, AddStructuralElementError> {
    if registries.structural().get_profile(profile).is_none() {
        return Err(AddStructuralElementError::UnknownProfile { profile });
    }
    if registries.materials().get_material(material).is_none() {
        return Err(AddStructuralElementError::UnknownMaterial { material });
    }
    geometry
        .validate()
        .map_err(AddStructuralElementError::Geometry)?;
    let structures = state.structures();
    let id = StructuralElementId::new(structures.next_element_id());
    let next_element_id = structures
        .next_element_id()
        .checked_add(1)
        .ok_or(AddStructuralElementError::IdExhausted)?;
    let next_revision = structures
        .revision()
        .checked_add(1)
        .ok_or(AddStructuralElementError::RevisionExhausted)?;
    let record = StructuralElementRecord {
        id,
        configuration: StructuralElementConfiguration {
            profile,
            material,
            geometry,
            is_grounded,
        },
        embodied_mass: crate::core::quantity::Mass::ZERO,
        embodied_material: Vec::new(),
        loads: Default::default(),
        lifecycle: StructuralLifecycle::Planned,
        is_cracked: false,
        created_at: state.tick(),
    };

    let structures = state.structure_state_mut();
    structures.insert_element(record, next_element_id, next_revision);
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
                "structural element {} owns {} mg of embodied matter and must be deconstructed through a conserved recovery transaction",
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

    /// Commits the requested structural change and every resolved damage consequence atomically.
    pub fn commit(
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

fn apply_damage_events(
    structures: &mut super::state::StructureState,
    events: &[StructuralDamageEvent],
) {
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

/// Revision-bound batch of load contributions owned by one external subsystem.
///
/// The entire batch is analyzed and committed under one structural revision so a cross-owner
/// transaction never exposes an impossible intermediate load arrangement.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ValidatedStructuralLoadBatch {
    kind: StructuralLoadKind,
    loads: BTreeMap<StructuralElementId, Force>,
    expected_revision: u64,
    next_revision: u64,
    analysis: StructuralAnalysis,
}

impl ValidatedStructuralLoadBatch {
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

fn apply_owned_loads(
    structures: &mut super::state::StructureState,
    kind: StructuralLoadKind,
    loads: BTreeMap<StructuralElementId, Force>,
) {
    for (element, load) in loads {
        structures.set_load(element, kind, load);
    }
}

/// Revision-bound removal analyzed together with an external subsystem's final load contributions.
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ValidatedStructuralRemovalWithLoads {
    element: StructuralElementId,
    kind: StructuralLoadKind,
    loads: BTreeMap<StructuralElementId, Force>,
    expected_revision: u64,
    next_revision: u64,
    analysis: StructuralAnalysis,
}

impl ValidatedStructuralRemovalWithLoads {
    #[must_use]
    pub(crate) const fn analysis(&self) -> &StructuralAnalysis {
        &self.analysis
    }

    pub(crate) const fn expected_revision(&self) -> u64 {
        self.expected_revision
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
        validate_operation_commit_state(
            structures,
            StructuralMutation::RemoveElement {
                element: self.element,
            },
        )?;
        for element in self.loads.keys().copied() {
            if element == self.element || structures.get_element(element).is_none() {
                return Err(StructuralCommitError::StateChanged { element });
            }
        }
        for event in self.analysis.damage_events() {
            if event.element() == self.element || structures.get_element(event.element()).is_none()
            {
                return Err(StructuralCommitError::StateChanged {
                    element: event.element(),
                });
            }
        }

        apply_operation_unchecked(
            structures,
            StructuralMutation::RemoveElement {
                element: self.element,
            },
        );
        apply_owned_loads(structures, self.kind, self.loads);
        apply_damage_events(structures, self.analysis.damage_events());
        structures.apply_revision(self.next_revision);
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
            let Some(supports) = structures.support_set(element) else {
                return Err(StructuralCommitError::StateChanged { element });
            };
            let Some(dependents) = structures.dependent_set(support) else {
                return Err(StructuralCommitError::StateChanged { element: support });
            };
            if supports.contains(&support) || dependents.contains(&element) {
                return Err(StructuralCommitError::SupportStateChanged { element, support });
            }
        }
        StructuralMutation::RemoveSupport { element, support } => {
            let Some(supports) = structures.support_set(element) else {
                return Err(StructuralCommitError::StateChanged { element });
            };
            let Some(dependents) = structures.dependent_set(support) else {
                return Err(StructuralCommitError::StateChanged { element: support });
            };
            if !supports.contains(&support) || !dependents.contains(&element) {
                return Err(StructuralCommitError::SupportStateChanged { element, support });
            }
        }
        StructuralMutation::RemoveElement { element } => {
            if structures.get_element(element).is_none() {
                return Err(StructuralCommitError::StateChanged { element });
            }
            let Some(supports) = structures.support_set(element) else {
                return Err(StructuralCommitError::StateChanged { element });
            };
            let Some(dependents) = structures.dependent_set(element) else {
                return Err(StructuralCommitError::StateChanged { element });
            };
            for support in supports {
                let Some(reverse) = structures.dependent_set(*support) else {
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
                let Some(forward) = structures.support_set(*dependent) else {
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
            if structures.get_element(element).is_none() {
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
            structures.link_support(element, support);
        }
        StructuralMutation::RemoveSupport { element, support } => {
            structures.unlink_support(element, support);
        }
        StructuralMutation::RemoveElement { element } => {
            structures.remove_element(element);
        }
        StructuralMutation::Activate { element } => {
            structures.activate_element(element);
        }
        StructuralMutation::SetLoadContribution {
            element,
            kind,
            load,
        } => {
            structures.set_load(element, kind, load);
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
        state.structures(),
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

/// Validates structural removal together with final external load values under one revision.
///
/// This is used by cross-owner recovery operations where removing a member and depositing recovered
/// matter can affect the same structural component. Analysis therefore sees both consequences at
/// once rather than depending on an arbitrary intermediate mutation order.
pub(crate) fn validate_remove_structural_element_with_owned_loads(
    registries: &Registries,
    state: &AppState,
    element: StructuralElementId,
    kind: StructuralLoadKind,
    loads: BTreeMap<StructuralElementId, Force>,
) -> Result<ValidatedStructuralRemovalWithLoads, StructuralMutationError> {
    if state.structures().get_element(element).is_none() {
        return Err(StructuralMutationError::UnknownElement { element });
    }
    if let Some(equipment) = state.equipment().supported_equipment(element).next() {
        return Err(StructuralMutationError::ElementSupportsEquipment { element, equipment });
    }
    if let Some(stockpile) = state.inventory().supported_stockpiles(element).next() {
        return Err(StructuralMutationError::ElementSupportsStockpile { element, stockpile });
    }
    if let Some(store) = state.fluid().supported_stores(element).next() {
        return Err(StructuralMutationError::ElementSupportsFluidStore { element, store });
    }

    let mut changed = BTreeMap::new();
    for (load_element, load) in loads {
        if load_element == element {
            return Err(StructuralMutationError::LoadTargetsRemovedElement { element, kind });
        }
        let record = state.structures().get_element(load_element).ok_or(
            StructuralMutationError::UnknownElement {
                element: load_element,
            },
        )?;
        if record.load(kind) != load {
            changed.insert(load_element, load);
        }
    }

    let expected_revision = state.structures().revision();
    let next_revision = expected_revision
        .checked_add(1)
        .ok_or(StructuralMutationError::RevisionExhausted)?;
    let overlay = StructuralAnalysisOverlay::remove_element_with_loads(
        element,
        changed
            .iter()
            .map(|(load_element, load)| ((*load_element, kind), *load))
            .collect(),
    );
    let mut seeds = BTreeSet::new();
    if let Some(supports) = state.structures().supports(element) {
        seeds.extend(supports);
    }
    if let Some(dependents) = state.structures().dependents(element) {
        seeds.extend(dependents);
    }
    seeds.extend(changed.keys().copied());
    let analysis = analyze_structure_components_with_overlay(
        registries.structural(),
        registries.materials(),
        state.structures(),
        overlay,
        &seeds,
    )
    .map_err(StructuralMutationError::Analysis)?;

    Ok(ValidatedStructuralRemovalWithLoads {
        element,
        kind,
        loads: changed,
        expected_revision,
        next_revision,
        analysis,
    })
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

/// Validates several load contributions owned by one external subsystem as one structural change.
///
/// Entries already equal to authoritative state are omitted. If every requested load already
/// matches, no structural revision is required and this returns `None`.
pub(crate) fn validate_set_owned_structural_loads(
    registries: &Registries,
    state: &AppState,
    kind: StructuralLoadKind,
    loads: BTreeMap<StructuralElementId, Force>,
) -> Result<Option<ValidatedStructuralLoadBatch>, StructuralMutationError> {
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
        return Ok(None);
    }

    let expected_revision = state.structures().revision();
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

    Ok(Some(ValidatedStructuralLoadBatch {
        kind,
        loads: changed,
        expected_revision,
        next_revision,
        analysis,
    }))
}

#[cfg(all(
    test,
    any(not(feature = "test-unit-sharded"), feature = "test-unit-resources")
))]
mod tests {
    use super::*;
    use crate::content::{
        FORM_LOG, MATERIAL_WOOD, STRUCTURAL_PROFILE_AXIAL_COMPRESSION, build_registries,
    };
    use crate::core::quantity::{Area, Length};
    use crate::core::time::WorldSeed;
    use crate::inventory::add_solid_stockpile_for_test;
    use crate::spatial::{VoxelBounds, VoxelCoord};
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

    #[test]
    fn structural_geometry_rejects_zero_length_before_allocation() {
        assert_eq!(
            StructuralElementGeometry::new(make_test_bounds(0, 0), Length::ZERO, MEMBER_AREA),
            Err(crate::structural::StructuralGeometryError::ZeroLength)
        );
        assert_eq!(
            StructuralElementGeometry::new(
                make_test_bounds(0, 0),
                Length::from_micrometers(1),
                Area::ZERO,
            ),
            Err(crate::structural::StructuralGeometryError::ZeroCrossSection)
        );

        let valid = crate::structural::make_test_structural_geometry(
            make_test_bounds(0, 0),
            Length::from_micrometers(1),
            MEMBER_AREA,
        );
        let mut encoded = match serde_json::to_value(valid) {
            Ok(encoded) => encoded,
            Err(error) => panic!("structural geometry serialization failed: {error}"),
        };
        encoded["length"] = serde_json::json!(0_u64);
        assert!(serde_json::from_value::<StructuralElementGeometry>(encoded).is_err());
    }

    #[test]
    fn allocation_revalidates_geometry_before_mutating_state() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x5700_1002));
        let invalid = StructuralElementGeometry {
            bounds: make_test_bounds(0, 0),
            length: Length::ZERO,
            cross_section: MEMBER_AREA,
        };
        let before = state.clone();

        assert_eq!(
            add_structural_element(
                &registries,
                &mut state,
                STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
                MATERIAL_WOOD,
                invalid,
                true,
            ),
            Err(AddStructuralElementError::Geometry(
                crate::structural::StructuralGeometryError::ZeroLength
            ))
        );
        assert_eq!(state, before);
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
        let destination = match add_solid_stockpile_for_test(state, mass) {
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
        is_grounded: bool,
    ) -> StructuralElementId {
        let element = match add_structural_element(
            registries,
            state,
            STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
            MATERIAL_WOOD,
            crate::structural::make_test_structural_geometry(
                make_test_bounds(x, y),
                Length::from_micrometers(1),
                MEMBER_AREA,
            ),
            is_grounded,
        ) {
            Ok(element) => element,
            Err(error) => panic!("structural element fixture failed: {error}"),
        };
        materialize_structural_element_for_test(registries, state, element, FORM_LOG);
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

        let mut unrelated_elements = Vec::with_capacity(256);
        for index in 0_i64..256 {
            let unrelated = make_test_element(&registries, &mut state, 10 + index, 0, true);
            activate_test_element(&registries, &mut state, unrelated);
            unrelated_elements.push(unrelated);
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
        assert!(unrelated_elements.into_iter().all(|element| {
            state
                .structures()
                .get_element(element)
                .is_some_and(|record| record.lifecycle() == StructuralLifecycle::Active)
        }));
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
    fn unchanged_public_load_is_rejected_without_revision_churn() {
        let registries = build_registries();
        let mut state = AppState::new(WorldSeed::new(0x5100_0010));
        let member = make_test_element(&registries, &mut state, 0, 0, true);
        activate_test_element(&registries, &mut state, member);
        let load = Force::from_millinewtons(1_000_000);
        let initial = validate_set_structural_load(
            &registries,
            &state,
            member,
            StructuralLoadKind::Permanent,
            load,
        )
        .unwrap_or_else(|error| panic!("initial public load validation failed: {error}"));
        commit_test_mutation(initial, &mut state);
        let before = state.clone();

        assert_eq!(
            validate_set_structural_load(
                &registries,
                &state,
                member,
                StructuralLoadKind::Permanent,
                load,
            ),
            Err(StructuralMutationError::LoadUnchanged {
                element: member,
                kind: StructuralLoadKind::Permanent,
                load,
            })
        );
        assert_eq!(state, before);
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
