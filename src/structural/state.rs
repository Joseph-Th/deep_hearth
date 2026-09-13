//! Owns persistent structural members, support indexes, and local state validation.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::core::quantity::Force;
#[cfg(any(test, feature = "test-gameplay"))]
use crate::inventory::ConsumedMaterialTrace;

mod records;

#[cfg(any(test, feature = "test-gameplay"))]
pub(super) use records::StructuralElementConfiguration;
pub use records::{
    StructuralElementGeometry, StructuralElementId, StructuralElementRecord, StructuralLifecycle,
    StructuralLoadKind,
};

/// Authoritative structural records plus synchronized forward and reverse support indexes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StructureState {
    revision: u64,
    next_element_id: u32,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_btree_map_no_duplicates")]
    elements: BTreeMap<StructuralElementId, StructuralElementRecord>,
    #[serde(
        deserialize_with = "crate::core::serialization::deserialize_btree_map_of_sets_no_duplicates"
    )]
    supports_by_element: BTreeMap<StructuralElementId, BTreeSet<StructuralElementId>>,
    #[serde(skip)]
    dependents_by_support: BTreeMap<StructuralElementId, BTreeSet<StructuralElementId>>,
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
    #[cfg(any(test, feature = "test-gameplay"))]
    pub(super) const fn next_element_id(&self) -> u32 {
        self.next_element_id
    }

    #[must_use]
    pub fn get_element(&self, id: StructuralElementId) -> Option<&StructuralElementRecord> {
        self.elements.get(&id)
    }

    pub fn elements(&self) -> impl Iterator<Item = &StructuralElementRecord> {
        self.elements.values()
    }

    pub(crate) fn rebuild_derived_indexes(&mut self) {
        let mut dependents_by_support = self
            .elements
            .keys()
            .copied()
            .map(|element| (element, BTreeSet::new()))
            .collect::<BTreeMap<_, _>>();
        for (element, supports) in &self.supports_by_element {
            for support in supports {
                dependents_by_support
                    .entry(*support)
                    .or_default()
                    .insert(*element);
            }
        }
        self.dependents_by_support = dependents_by_support;
    }

    pub(super) const fn element_map(
        &self,
    ) -> &BTreeMap<StructuralElementId, StructuralElementRecord> {
        &self.elements
    }

    pub fn supports(
        &self,
        id: StructuralElementId,
    ) -> Option<impl Iterator<Item = StructuralElementId> + '_> {
        self.supports_by_element
            .get(&id)
            .map(|entries| entries.iter().copied())
    }

    pub(super) fn element_ids(&self) -> impl Iterator<Item = StructuralElementId> + '_ {
        self.elements.keys().copied()
    }

    #[must_use]
    pub(super) fn support_set(
        &self,
        element: StructuralElementId,
    ) -> Option<&BTreeSet<StructuralElementId>> {
        self.supports_by_element.get(&element)
    }

    #[must_use]
    pub(super) fn dependent_set(
        &self,
        support: StructuralElementId,
    ) -> Option<&BTreeSet<StructuralElementId>> {
        self.dependents_by_support.get(&support)
    }

    #[cfg(any(test, feature = "test-gameplay"))]
    pub(super) fn insert_element(
        &mut self,
        record: StructuralElementRecord,
        next_element_id: u32,
        next_revision: u64,
    ) {
        let id = record.id;
        assert_eq!(
            id.value(),
            self.next_element_id,
            "structural allocation must consume the current identity cursor"
        );
        assert_eq!(
            self.next_element_id.checked_add(1),
            Some(next_element_id),
            "structural allocation must advance the identity cursor exactly once"
        );
        assert_eq!(
            self.revision.checked_add(1),
            Some(next_revision),
            "structural allocation must advance the owner revision exactly once"
        );
        let previous_record = self.elements.insert(id, record);
        let previous_supports = self.supports_by_element.insert(id, BTreeSet::new());
        let previous_dependents = self.dependents_by_support.insert(id, BTreeSet::new());
        assert!(
            previous_record.is_none()
                && previous_supports.is_none()
                && previous_dependents.is_none(),
            "Runtime Invariant 4 (Index Uniqueness): structural allocation replaced existing state"
        );
        self.next_element_id = next_element_id;
        self.revision = next_revision;
    }

    #[cfg(test)]
    pub(super) fn link_support(
        &mut self,
        element: StructuralElementId,
        support: StructuralElementId,
    ) {
        let inserted_support = self
            .supports_by_element
            .get_mut(&element)
            .unwrap_or_else(|| panic!("prevalidated structural support source disappeared"))
            .insert(support);
        let inserted_dependent = self
            .dependents_by_support
            .get_mut(&support)
            .unwrap_or_else(|| panic!("prevalidated structural support target disappeared"))
            .insert(element);
        assert!(
            inserted_support && inserted_dependent,
            "prevalidated structural support edge already existed"
        );
    }

    #[cfg(test)]
    pub(super) fn unlink_support(
        &mut self,
        element: StructuralElementId,
        support: StructuralElementId,
    ) {
        let removed_support = self
            .supports_by_element
            .get_mut(&element)
            .unwrap_or_else(|| panic!("prevalidated structural support source disappeared"))
            .remove(&support);
        let removed_dependent = self
            .dependents_by_support
            .get_mut(&support)
            .unwrap_or_else(|| panic!("prevalidated structural support target disappeared"))
            .remove(&element);
        assert!(
            removed_support && removed_dependent,
            "prevalidated structural support edge disappeared"
        );
    }

    #[cfg(test)]
    pub(super) fn remove_element(&mut self, element: StructuralElementId) {
        let supports = self
            .supports_by_element
            .get(&element)
            .cloned()
            .unwrap_or_else(|| panic!("prevalidated removed element lost support index"));
        let dependents = self
            .dependents_by_support
            .get(&element)
            .cloned()
            .unwrap_or_else(|| panic!("prevalidated removed element lost dependent index"));
        for support in supports {
            let removed = self
                .dependents_by_support
                .get_mut(&support)
                .unwrap_or_else(|| panic!("prevalidated structural reverse index disappeared"))
                .remove(&element);
            assert!(removed, "prevalidated structural reverse edge disappeared");
        }
        for dependent in dependents {
            let removed = self
                .supports_by_element
                .get_mut(&dependent)
                .unwrap_or_else(|| panic!("prevalidated structural forward index disappeared"))
                .remove(&element);
            assert!(removed, "prevalidated structural forward edge disappeared");
        }
        assert!(self.supports_by_element.remove(&element).is_some());
        assert!(self.dependents_by_support.remove(&element).is_some());
        assert!(self.elements.remove(&element).is_some());
    }

    #[cfg(any(test, feature = "test-gameplay"))]
    pub(super) fn activate_element(&mut self, element: StructuralElementId) {
        self.elements
            .get_mut(&element)
            .unwrap_or_else(|| panic!("prevalidated structural activation target disappeared"))
            .lifecycle = StructuralLifecycle::Active;
    }

    pub(super) fn set_load(
        &mut self,
        element: StructuralElementId,
        kind: StructuralLoadKind,
        load: Force,
    ) {
        let record = self
            .elements
            .get_mut(&element)
            .unwrap_or_else(|| panic!("prevalidated structural load target disappeared"));
        if load.is_zero() {
            record.loads.remove(&kind);
        } else {
            record.loads.insert(kind, load);
        }
    }

    pub(super) fn apply_damage(&mut self, element: StructuralElementId, failed: bool) {
        let record = self
            .elements
            .get_mut(&element)
            .unwrap_or_else(|| panic!("prevalidated structural damage target disappeared"));
        record.is_cracked = true;
        if failed {
            record.lifecycle = StructuralLifecycle::Failed;
        }
    }

    #[cfg(any(test, feature = "test-gameplay"))]
    pub(super) fn set_embodied_matter(
        &mut self,
        element: StructuralElementId,
        material: Vec<ConsumedMaterialTrace>,
        self_weight: Force,
    ) {
        let record = self
            .elements
            .get_mut(&element)
            .unwrap_or_else(|| panic!("prechecked structural construction target disappeared"));
        record.embodied_material = material;
        if self_weight.is_zero() {
            record.loads.remove(&StructuralLoadKind::SelfWeight);
        } else {
            record
                .loads
                .insert(StructuralLoadKind::SelfWeight, self_weight);
        }
    }

    pub(super) fn apply_revision(&mut self, next_revision: u64) {
        assert_eq!(
            self.revision.checked_add(1),
            Some(next_revision),
            "structural revision must advance exactly once per canonical mutation batch"
        );
        self.revision = next_revision;
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
        let mut pending = vec![from];
        let mut visited = BTreeSet::new();
        while let Some(current) = pending.pop() {
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

mod validation;

pub use validation::StructureValidationError;
pub(crate) use validation::validate_loaded_structure;
