//! Persistent structural members and synchronized support indexes; sibling execution owns every mutation path.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::core::quantity::{Area, Force};
use crate::core::time::SimulationTick;
use crate::material::{MaterialId, MaterialRegistry};
use crate::spatial::VoxelBounds;

use super::definitions::{StructuralProfileId, StructuralRegistry};

/// Persistent identifier for one structural member record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct StructuralElementId(u32);

impl StructuralElementId {
    #[must_use]
    pub const fn new(value: u32) -> Self {
        assert!(value != 0, "structural element id must be nonzero");
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }
}

/// Structural lifecycle separates construction configuration from load-bearing participation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum StructuralLifecycle {
    Planned,
    Active,
    Failed,
}

/// Physical origin of an externally resolved load contribution.
///
/// Each owning system updates only its own contribution so unrelated causes cannot overwrite one
/// another. Structural analysis consumes the sum and does not invent these source values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum StructuralLoadKind {
    Permanent,
    StoredMatter,
    Equipment,
    Fluid,
    Snow,
    Wind,
    Occupancy,
}

/// Persistent physical and lifecycle state for one structural member.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructuralElementRecord {
    pub(super) id: StructuralElementId,
    pub(super) profile: StructuralProfileId,
    pub(super) material: MaterialId,
    pub(super) bounds: VoxelBounds,
    pub(super) cross_section: Area,
    pub(super) grounded: bool,
    pub(super) loads: BTreeMap<StructuralLoadKind, Force>,
    pub(super) lifecycle: StructuralLifecycle,
    pub(super) cracked: bool,
    pub(super) created_at: SimulationTick,
}

impl StructuralElementRecord {
    #[must_use]
    pub const fn id(&self) -> StructuralElementId {
        self.id
    }

    #[must_use]
    pub const fn profile(&self) -> StructuralProfileId {
        self.profile
    }

    #[must_use]
    pub const fn material(&self) -> MaterialId {
        self.material
    }

    #[must_use]
    pub const fn bounds(&self) -> VoxelBounds {
        self.bounds
    }

    #[must_use]
    pub const fn cross_section(&self) -> Area {
        self.cross_section
    }

    #[must_use]
    pub const fn is_grounded(&self) -> bool {
        self.grounded
    }

    #[must_use]
    pub fn load(&self, kind: StructuralLoadKind) -> Force {
        self.loads.get(&kind).copied().unwrap_or(Force::ZERO)
    }

    pub fn loads(&self) -> impl Iterator<Item = (StructuralLoadKind, Force)> + '_ {
        self.loads.iter().map(|(kind, force)| (*kind, *force))
    }

    #[must_use]
    pub const fn lifecycle(&self) -> StructuralLifecycle {
        self.lifecycle
    }

    #[must_use]
    pub const fn is_cracked(&self) -> bool {
        self.cracked
    }

    #[must_use]
    pub const fn created_at(&self) -> SimulationTick {
        self.created_at
    }
}

/// Authoritative structural records plus synchronized forward and reverse support indexes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructureState {
    pub(super) revision: u64,
    pub(super) next_element_id: u32,
    pub(super) elements: BTreeMap<StructuralElementId, StructuralElementRecord>,
    pub(super) supports_by_element: BTreeMap<StructuralElementId, BTreeSet<StructuralElementId>>,
    pub(super) dependents_by_support: BTreeMap<StructuralElementId, BTreeSet<StructuralElementId>>,
}

impl StructureState {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            revision: 0,
            next_element_id: 1,
            elements: BTreeMap::new(),
            supports_by_element: BTreeMap::new(),
            dependents_by_support: BTreeMap::new(),
        }
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub fn get_element(&self, id: StructuralElementId) -> Option<&StructuralElementRecord> {
        self.elements.get(&id)
    }

    pub fn elements(&self) -> impl Iterator<Item = &StructuralElementRecord> {
        self.elements.values()
    }

    pub fn supports(
        &self,
        id: StructuralElementId,
    ) -> Option<impl Iterator<Item = StructuralElementId> + '_> {
        self.supports_by_element
            .get(&id)
            .map(|entries| entries.iter().copied())
    }

    pub fn dependents(
        &self,
        id: StructuralElementId,
    ) -> Option<impl Iterator<Item = StructuralElementId> + '_> {
        self.dependents_by_support
            .get(&id)
            .map(|entries| entries.iter().copied())
    }

    pub(crate) fn has_valid_id_cursor(&self) -> bool {
        self.next_element_id != 0
            && self
                .elements
                .keys()
                .next_back()
                .is_none_or(|id| id.value() < self.next_element_id)
    }

    pub(crate) fn has_path(&self, from: StructuralElementId, target: StructuralElementId) -> bool {
        let mut pending = BTreeSet::from([from]);
        let mut visited = BTreeSet::new();
        while let Some(current) = pending.pop_first() {
            if current == target {
                return true;
            }
            if !visited.insert(current) {
                continue;
            }
            let Some(supports) = self.supports_by_element.get(&current) else {
                continue;
            };
            pending.extend(supports.iter().copied());
        }
        false
    }
}

/// Exhaustive failure found while validating decoded structural state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StructureValidationError {
    ZeroNextElementId,
    NextElementIdNotAboveAllocated {
        next: u32,
        highest: StructuralElementId,
    },
    ElementKeyMismatch {
        key: StructuralElementId,
        record: StructuralElementId,
    },
    UnknownProfile {
        element: StructuralElementId,
        profile: StructuralProfileId,
    },
    UnknownMaterial {
        element: StructuralElementId,
        material: MaterialId,
    },
    ZeroCrossSection {
        element: StructuralElementId,
    },
    ZeroLoadContribution {
        element: StructuralElementId,
        kind: StructuralLoadKind,
    },
    CreatedInFuture {
        element: StructuralElementId,
        created_at: SimulationTick,
        current: SimulationTick,
    },
    FailedElementNotCracked {
        element: StructuralElementId,
    },
    MissingSupportIndex {
        element: StructuralElementId,
    },
    OrphanSupportIndex {
        element: StructuralElementId,
    },
    UnknownSupportReference {
        element: StructuralElementId,
        support: StructuralElementId,
    },
    SelfSupport {
        element: StructuralElementId,
    },
    GroundedElementHasSupport {
        element: StructuralElementId,
        support: StructuralElementId,
    },
    ReverseIndexMismatch {
        element: StructuralElementId,
        support: StructuralElementId,
    },
    SupportCycle {
        element: StructuralElementId,
        support: StructuralElementId,
    },
    ActiveElementUnsupported {
        element: StructuralElementId,
    },
}

impl Display for StructureValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroNextElementId => {
                formatter.write_str("structural next-id cursor must be nonzero")
            }
            Self::NextElementIdNotAboveAllocated { next, highest } => write!(
                formatter,
                "structural next-id cursor {next} is not above allocated element {}",
                highest.value()
            ),
            Self::ElementKeyMismatch { key, record } => write!(
                formatter,
                "structural element map key {} disagrees with record id {}",
                key.value(),
                record.value()
            ),
            Self::UnknownProfile { element, profile } => write!(
                formatter,
                "structural element {} references unknown profile {}",
                element.value(),
                profile.value()
            ),
            Self::UnknownMaterial { element, material } => write!(
                formatter,
                "structural element {} references unknown material {}",
                element.value(),
                material.value()
            ),
            Self::ZeroCrossSection { element } => write!(
                formatter,
                "structural element {} has zero cross-sectional area",
                element.value()
            ),
            Self::ZeroLoadContribution { element, kind } => write!(
                formatter,
                "structural element {} stores redundant zero {kind:?} load contribution",
                element.value()
            ),
            Self::CreatedInFuture {
                element,
                created_at,
                current,
            } => write!(
                formatter,
                "structural element {} was created at tick {} after current tick {}",
                element.value(),
                created_at.value(),
                current.value()
            ),
            Self::FailedElementNotCracked { element } => write!(
                formatter,
                "failed structural element {} is missing persistent crack damage",
                element.value()
            ),
            Self::MissingSupportIndex { element } => write!(
                formatter,
                "structural element {} is missing synchronized support index entries",
                element.value()
            ),
            Self::OrphanSupportIndex { element } => write!(
                formatter,
                "structural support index contains missing element {}",
                element.value()
            ),
            Self::UnknownSupportReference { element, support } => write!(
                formatter,
                "structural element {} references missing support {}",
                element.value(),
                support.value()
            ),
            Self::SelfSupport { element } => write!(
                formatter,
                "structural element {} cannot support itself",
                element.value()
            ),
            Self::GroundedElementHasSupport { element, support } => write!(
                formatter,
                "ground-anchored structural element {} cannot also route load through support {}",
                element.value(),
                support.value()
            ),
            Self::ReverseIndexMismatch { element, support } => write!(
                formatter,
                "structural support indexes disagree for element {} and support {}",
                element.value(),
                support.value()
            ),
            Self::SupportCycle { element, support } => write!(
                formatter,
                "structural support edge {} -> {} participates in a cycle",
                element.value(),
                support.value()
            ),
            Self::ActiveElementUnsupported { element } => write!(
                formatter,
                "active structural element {} has no active support or ground anchor",
                element.value()
            ),
        }
    }
}

impl Error for StructureValidationError {}

pub(crate) fn validate_loaded_structure(
    profiles: &StructuralRegistry,
    materials: &MaterialRegistry,
    state: &StructureState,
    current_tick: SimulationTick,
) -> Result<(), StructureValidationError> {
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

    for (id, record) in &state.elements {
        if *id != record.id {
            return Err(StructureValidationError::ElementKeyMismatch {
                key: *id,
                record: record.id,
            });
        }
        if profiles.get_profile(record.profile).is_none() {
            return Err(StructureValidationError::UnknownProfile {
                element: record.id,
                profile: record.profile,
            });
        }
        if materials.get_material(record.material).is_none() {
            return Err(StructureValidationError::UnknownMaterial {
                element: record.id,
                material: record.material,
            });
        }
        if record.cross_section.is_zero() {
            return Err(StructureValidationError::ZeroCrossSection { element: record.id });
        }
        if let Some((kind, _)) = record.loads.iter().find(|(_, load)| load.is_zero()) {
            return Err(StructureValidationError::ZeroLoadContribution {
                element: record.id,
                kind: *kind,
            });
        }
        if record.created_at > current_tick {
            return Err(StructureValidationError::CreatedInFuture {
                element: record.id,
                created_at: record.created_at,
                current: current_tick,
            });
        }
        if record.lifecycle == StructuralLifecycle::Failed && !record.cracked {
            return Err(StructureValidationError::FailedElementNotCracked { element: record.id });
        }
        if !state.supports_by_element.contains_key(id)
            || !state.dependents_by_support.contains_key(id)
        {
            return Err(StructureValidationError::MissingSupportIndex { element: *id });
        }
    }

    for id in state
        .supports_by_element
        .keys()
        .chain(state.dependents_by_support.keys())
    {
        if !state.elements.contains_key(id) {
            return Err(StructureValidationError::OrphanSupportIndex { element: *id });
        }
    }

    for (element, supports) in &state.supports_by_element {
        for support in supports {
            if element == support {
                return Err(StructureValidationError::SelfSupport { element: *element });
            }
            if state.elements[element].grounded {
                return Err(StructureValidationError::GroundedElementHasSupport {
                    element: *element,
                    support: *support,
                });
            }
            if !state.elements.contains_key(support) {
                return Err(StructureValidationError::UnknownSupportReference {
                    element: *element,
                    support: *support,
                });
            }
            if !state
                .dependents_by_support
                .get(support)
                .is_some_and(|dependents| dependents.contains(element))
            {
                return Err(StructureValidationError::ReverseIndexMismatch {
                    element: *element,
                    support: *support,
                });
            }
            if state.has_path(*support, *element) {
                return Err(StructureValidationError::SupportCycle {
                    element: *element,
                    support: *support,
                });
            }
        }
    }
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

    for record in state.elements.values() {
        if record.lifecycle != StructuralLifecycle::Active || record.grounded {
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
