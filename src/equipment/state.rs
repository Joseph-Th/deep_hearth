//! Owns persistent equipment records, embodiment, support assignment, and synchronized mutations.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::core::quantity::Mass;
use crate::structural::{
    StructuralElementId, apply_support_index_change, assert_support_index_change_available,
};

mod condition;
mod record;

pub(super) use record::{EquipmentComponentMaintenanceMutation, EquipmentUpgradeMutation};
pub use record::{EquipmentId, EquipmentOperationTrace, EquipmentRecord};
pub(crate) use record::{EquipmentMaintenanceAdmission, EquipmentOperationConditionOutcome};

/// Authoritative equipment collection and monotonic mutation/version state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EquipmentState {
    revision: u64,
    support_revision: u64,
    next_equipment_id: u32,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_btree_map_no_duplicates")]
    records: BTreeMap<EquipmentId, EquipmentRecord>,
    #[serde(skip)]
    equipment_by_support: BTreeMap<StructuralElementId, BTreeSet<EquipmentId>>,
}

impl EquipmentState {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            revision: 0,
            support_revision: 0,
            next_equipment_id: 1,
            records: BTreeMap::new(),
            equipment_by_support: BTreeMap::new(),
        }
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Monotonic epoch for equipment support-assignment changes only.
    #[must_use]
    pub(crate) const fn support_revision(&self) -> u64 {
        self.support_revision
    }

    #[must_use]
    pub(super) const fn next_equipment_id(&self) -> u32 {
        self.next_equipment_id
    }

    #[must_use]
    pub fn get_equipment(&self, id: EquipmentId) -> Option<&EquipmentRecord> {
        self.records.get(&id)
    }

    pub fn equipment(&self) -> impl Iterator<Item = &EquipmentRecord> {
        self.records.values()
    }

    pub(super) fn assert_allocation_available(
        &self,
        record: &EquipmentRecord,
        next_equipment_id: u32,
        next_revision: u64,
    ) {
        assert_eq!(
            record.id.value(),
            self.next_equipment_id,
            "equipment allocation must consume the current identity cursor"
        );
        assert_eq!(
            self.next_equipment_id.checked_add(1),
            Some(next_equipment_id),
            "equipment allocation must advance the identity cursor exactly once"
        );
        assert_eq!(
            self.revision.checked_add(1),
            Some(next_revision),
            "equipment allocation must advance the owner revision exactly once"
        );
        assert!(
            !self.records.contains_key(&record.id),
            "Runtime Invariant 4 (Index Uniqueness): equipment allocation replaced an existing id"
        );
    }

    pub(crate) fn rebuild_derived_indexes(&mut self) {
        let mut equipment_by_support =
            BTreeMap::<StructuralElementId, BTreeSet<EquipmentId>>::new();
        for (equipment, record) in &self.records {
            if let Some(support) = record.supported_by {
                equipment_by_support
                    .entry(support)
                    .or_default()
                    .insert(*equipment);
            }
        }
        self.equipment_by_support = equipment_by_support;
    }

    /// Atomically inserts one allocated equipment record and advances identity and revision cursors.
    pub(super) fn insert_equipment(
        &mut self,
        record: EquipmentRecord,
        next_equipment_id: u32,
        next_revision: u64,
    ) {
        self.assert_allocation_available(&record, next_equipment_id, next_revision);
        let previous = self.records.insert(record.id, record);
        assert!(
            previous.is_none(),
            "prechecked equipment insertion unexpectedly replaced a record"
        );
        self.next_equipment_id = next_equipment_id;
        self.revision = next_revision;
    }

    pub(super) fn assert_upgrade_available(
        &self,
        mutation: &EquipmentUpgradeMutation,
        expected_revision: u64,
        next_revision: u64,
    ) {
        assert_eq!(self.revision, expected_revision);
        assert_eq!(expected_revision.checked_add(1), Some(next_revision));
        assert!(
            !mutation.additions.is_empty(),
            "equipment upgrade additions must be nonempty"
        );
        let record = self.records.get(&mutation.equipment).unwrap_or_else(|| {
            panic!(
                "runtime invariant broken: equipment {} disappeared before upgrade",
                mutation.equipment.value()
            )
        });
        assert_eq!(record.definition, mutation.expected_definition);
        assert_eq!(record.embodied_mass, mutation.expected_embodied_mass);
        assert!(
            record.supported_by.is_none(),
            "validated equipment upgrade cannot mutate mounted equipment"
        );
        let added_mass = mutation.additions.iter().fold(Mass::ZERO, |total, trace| {
            total
                .checked_add(trace.mass())
                .unwrap_or_else(|| panic!("validated equipment upgrade addition mass overflowed"))
        });
        assert_eq!(
            mutation.expected_embodied_mass.checked_add(added_mass),
            Some(mutation.target_embodied_mass)
        );
    }

    /// Applies one additive equipment-definition upgrade without replacing runtime identity.
    pub(super) fn apply_upgrade(
        &mut self,
        mutation: EquipmentUpgradeMutation,
        expected_revision: u64,
        next_revision: u64,
    ) {
        self.assert_upgrade_available(&mutation, expected_revision, next_revision);
        let record = self
            .records
            .get_mut(&mutation.equipment)
            .unwrap_or_else(|| unreachable!("equipment upgrade record was prechecked"));
        record.definition = mutation.target_definition;
        record.embodied_mass = mutation.target_embodied_mass;
        record.embodied_material.extend(mutation.additions);
        self.revision = next_revision;
    }

    pub(super) fn assert_removal_available(
        &self,
        equipment: EquipmentId,
        expected_revision: u64,
        next_revision: u64,
    ) {
        assert_eq!(self.revision, expected_revision);
        assert_eq!(expected_revision.checked_add(1), Some(next_revision));
        let record = self.records.get(&equipment).unwrap_or_else(|| {
            panic!(
                "runtime invariant broken: equipment {} disappeared before disassembly",
                equipment.value()
            )
        });
        assert!(
            record.supported_by.is_none(),
            "validated equipment disassembly cannot remove mounted equipment"
        );
        assert!(
            self.equipment_by_support
                .values()
                .all(|supported| !supported.contains(&equipment)),
            "validated unmounted equipment remained in the support reverse index"
        );
    }

    /// Removes one prevalidated unmounted equipment instance without rewinding its ID cursor.
    pub(super) fn remove_equipment(
        &mut self,
        equipment: EquipmentId,
        expected_revision: u64,
        next_revision: u64,
    ) {
        self.assert_removal_available(equipment, expected_revision, next_revision);
        assert!(self.records.remove(&equipment).is_some());
        self.revision = next_revision;
    }

    /// Iterates equipment assigned to one structural support in stable equipment-ID order.
    pub(crate) fn supported_equipment(
        &self,
        support: StructuralElementId,
    ) -> impl Iterator<Item = EquipmentId> + '_ {
        self.equipment_by_support
            .get(&support)
            .into_iter()
            .flat_map(|equipment| equipment.iter().copied())
    }

    pub(super) fn assert_support_change_available(
        &self,
        equipment: EquipmentId,
        before: Option<StructuralElementId>,
        after: Option<StructuralElementId>,
        next_revision: u64,
    ) {
        assert_eq!(
            self.revision.checked_add(1),
            Some(next_revision),
            "validated equipment support change must advance the owner revision exactly once"
        );
        let record = match self.records.get(&equipment) {
            Some(record) => record,
            None => panic!(
                "runtime invariant broken: equipment {} disappeared during support update",
                equipment.value()
            ),
        };
        assert_eq!(
            record.supported_by, before,
            "runtime invariant broken: equipment support record disagrees with support index"
        );
        assert_support_index_change_available(&self.equipment_by_support, equipment, before, after);
    }

    pub(super) fn apply_support_change(
        &mut self,
        equipment: EquipmentId,
        before: Option<StructuralElementId>,
        after: Option<StructuralElementId>,
        next_revision: u64,
    ) {
        self.assert_support_change_available(equipment, before, after, next_revision);
        apply_support_index_change(&mut self.equipment_by_support, equipment, before, after);
        let record = match self.records.get_mut(&equipment) {
            Some(record) => record,
            None => unreachable!("equipment support record was prechecked before index mutation"),
        };
        record.supported_by = after;
        self.support_revision = next_revision;
        self.revision = next_revision;
    }

    pub(crate) fn has_valid_id_cursor(&self) -> bool {
        self.next_equipment_id != 0
            && self
                .records
                .keys()
                .next_back()
                .is_none_or(|id| id.value() < self.next_equipment_id)
    }
}

mod validation;

pub use validation::EquipmentValidationError;
pub(crate) use validation::validate_loaded_equipment;
