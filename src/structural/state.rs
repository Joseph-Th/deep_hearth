//! Owns structural members, topology, derived local geometry indexes, and validation.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::core::quantity::Force;
use crate::inventory::ConsumedMaterialTrace;
use crate::spatial::{VoxelBounds, VoxelCoord};

mod records;
mod spatial_index;

use spatial_index::{ElementSpatialIndex, occupied_voxels};

const MAX_INDEXED_INTERSECTION_VOXELS: u128 = 64;

pub(super) use records::StructuralElementConfiguration;
pub use records::{
    StructuralElementGeometry, StructuralElementId, StructuralElementRecord, StructuralLifecycle,
    StructuralLoadKind,
};

/// Authoritative members, persisted support topology, and rebuilt local geometry indexes.
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
    #[serde(skip)]
    spatial_index: ElementSpatialIndex,
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
            spatial_index: ElementSpatialIndex::new(),
        }
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
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

    /// Inspect only structural members whose bounds cover this voxel.
    ///
    /// This derived index is rebuilt from authoritative records after trusted load.
    /// Broad geometries use a bounded-memory fallback with an exact bounds test.
    pub(crate) fn elements_covering(
        &self,
        voxel: VoxelCoord,
    ) -> impl Iterator<Item = &StructuralElementRecord> {
        self.spatial_index
            .candidates(voxel)
            .map(|id| {
                self.elements
                    .get(&id)
                    .expect("structural spatial index references absent element")
            })
            .filter(move |element| element.bounds().has_voxel(voxel))
    }

    /// Tests active, full-footprint shelter on one horizontal sky-exposure column.
    ///
    /// Derived horizontal projection avoids scanning every settlement member on
    /// every slow survival tick. The caller supplies the generated sky limit.
    #[must_use]
    pub(crate) fn has_active_overhead_cover(
        &self,
        x: i64,
        z: i64,
        from_y: i64,
        sky_exclusive: i64,
    ) -> bool {
        self.spatial_index.column_candidates(x, z).any(|id| {
            let member = self
                .elements
                .get(&id)
                .expect("structural column index references absent element");
            member.lifecycle() == StructuralLifecycle::Active
                && member.geometry().covers_horizontal_footprint()
                && member.bounds().min().y() < sky_exclusive
                && member.bounds().max_exclusive().y() > from_y
        })
    }

    /// A materialized, full-footprint member occupies this voxel even before activation.
    /// Planned members have no physical obstruction.
    #[must_use]
    pub(crate) fn blocks_full_cell(&self, voxel: VoxelCoord) -> bool {
        self.elements_covering(voxel).any(|element| {
            !element.embodied_material().is_empty()
                && element.geometry().covers_horizontal_footprint()
        })
    }

    /// Any actual built member, including a narrow post, obstructs a heavy
    /// resident's occupied body volume. For small footprints, use local indexed
    /// occupancy; for unusually large bounds, scan each member just once rather
    /// than expanding an unbounded number of voxel lookups.
    #[must_use]
    pub(crate) fn has_embodied_member_intersecting(&self, bounds: VoxelBounds) -> bool {
        if bounds
            .voxel_count()
            .is_some_and(|count| count <= MAX_INDEXED_INTERSECTION_VOXELS)
        {
            return occupied_voxels(bounds).any(|voxel| {
                self.elements_covering(voxel)
                    .any(|element| !element.embodied_material().is_empty())
            });
        }
        self.elements.values().any(|element| {
            !element.embodied_material().is_empty() && element.bounds().has_intersection(bounds)
        })
    }

    /// Identifies another embodied full-footprint member occupying the same voxel.
    /// Narrow framing and inert plans can overlap a panel under the authored joint model.
    pub(crate) fn overlapping_embodied_full_footprint(
        &self,
        candidate: &StructuralElementRecord,
    ) -> Option<(StructuralElementId, VoxelCoord)> {
        if !candidate.geometry().covers_horizontal_footprint() {
            return None;
        }
        self.elements.values().find_map(|member| {
            if member.id() == candidate.id()
                || member.embodied_material().is_empty()
                || !member.geometry().covers_horizontal_footprint()
                || !candidate.bounds().has_intersection(member.bounds())
            {
                return None;
            }
            let left = candidate.bounds().min();
            let right = member.bounds().min();
            Some((
                member.id(),
                VoxelCoord::new(
                    left.x().max(right.x()),
                    left.y().max(right.y()),
                    left.z().max(right.z()),
                ),
            ))
        })
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
        let mut spatial_index = ElementSpatialIndex::new();
        for (&id, record) in &self.elements {
            spatial_index.insert(id, record.bounds());
        }
        self.spatial_index = spatial_index;
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

    /// Whether a member bears any direct dependents, through the synchronized reverse index.
    pub(crate) fn has_dependents(&self, element: StructuralElementId) -> bool {
        self.dependents_by_support
            .get(&element)
            .is_some_and(|dependents| !dependents.is_empty())
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
        self.spatial_index.insert(id, record.bounds());
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
        let removed = self
            .elements
            .remove(&element)
            .expect("prevalidated structural element disappeared");
        self.spatial_index.remove(element, removed.bounds());
    }

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
