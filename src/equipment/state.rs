//! Owns persistent equipment records, embodiment, support assignment, and synchronized mutations.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::core::quantity::Mass;
use crate::maintenance::Condition;
use crate::structural::{
    StructuralElementId, apply_support_index_change, assert_support_index_change_available,
};

mod record;

pub(crate) use record::EquipmentOperationConditionOutcome;
pub(super) use record::{EquipmentComponentMaintenanceMutation, EquipmentUpgradeMutation};
pub use record::{EquipmentId, EquipmentOperationTrace, EquipmentRecord};

/// Authoritative equipment collection and monotonic mutation/version state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EquipmentState {
    revision: u64,
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
            next_equipment_id: 1,
            records: BTreeMap::new(),
            equipment_by_support: BTreeMap::new(),
        }
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
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
        for record in self.records.values() {
            if let Some(support) = record.supported_by {
                equipment_by_support
                    .entry(support)
                    .or_default()
                    .insert(record.id);
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

    pub(super) fn assert_component_maintenance_available(
        &self,
        mutation: &EquipmentComponentMaintenanceMutation,
        expected_revision: u64,
        next_revision: u64,
    ) {
        assert_eq!(self.revision, expected_revision);
        assert_eq!(expected_revision.checked_add(1), Some(next_revision));
        assert!(
            !mutation.replacement.is_empty(),
            "component replacement traces must be nonempty"
        );
        assert!(
            mutation
                .replacement
                .iter()
                .all(|trace| trace.profile().commodity() == mutation.component),
            "component replacement traces must match the authored component commodity"
        );

        let record = self.records.get(&mutation.equipment).unwrap_or_else(|| {
            panic!(
                "runtime invariant broken: equipment {} disappeared before component maintenance",
                mutation.equipment.value()
            )
        });
        assert_eq!(record.condition, mutation.condition_before);
        let replaced_mass = record
            .embodied_material
            .iter()
            .filter(|trace| trace.profile().commodity() == mutation.component)
            .fold(Mass::ZERO, |total, trace| {
                total.checked_add(trace.mass()).unwrap_or_else(|| {
                    panic!("validated embodied component mass overflowed during maintenance")
                })
            });
        let replacement_mass = mutation
            .replacement
            .iter()
            .fold(Mass::ZERO, |total, trace| {
                total.checked_add(trace.mass()).unwrap_or_else(|| {
                    panic!("validated replacement component mass overflowed during maintenance")
                })
            });
        assert!(
            !replaced_mass.is_zero(),
            "validated component must exist in equipment"
        );
        assert_eq!(replaced_mass, replacement_mass);
    }

    /// Exchanges every trace belonging to one authored component for exact fresh traces while
    /// preserving equipment identity, all unrelated embodied matter, and total embodied mass.
    pub(super) fn apply_component_maintenance(
        &mut self,
        mutation: EquipmentComponentMaintenanceMutation,
        expected_revision: u64,
        next_revision: u64,
    ) {
        self.assert_component_maintenance_available(&mutation, expected_revision, next_revision);
        let record = self
            .records
            .get_mut(&mutation.equipment)
            .unwrap_or_else(|| unreachable!("component-maintenance equipment was prechecked"));

        let mut inserted = false;
        let mut next_embodied =
            Vec::with_capacity(record.embodied_material.len() + mutation.replacement.len());
        for trace in record.embodied_material.drain(..) {
            if trace.profile().commodity() == mutation.component {
                if !inserted {
                    next_embodied.extend(mutation.replacement.iter().cloned());
                    inserted = true;
                }
            } else {
                next_embodied.push(trace);
            }
        }
        assert!(inserted);
        record.embodied_material = next_embodied;
        record.condition = mutation.condition_after;
        self.revision = next_revision;
    }

    pub(crate) fn assert_condition_change_available(
        &self,
        equipment: EquipmentId,
        before: Condition,
        next_revision: u64,
    ) {
        assert_eq!(
            self.revision.checked_add(1),
            Some(next_revision),
            "equipment condition mutation must advance revision exactly once"
        );
        let record = self.records.get(&equipment).unwrap_or_else(|| {
            panic!(
                "runtime invariant broken: equipment {} disappeared before condition update",
                equipment.value()
            )
        });
        assert_eq!(
            record.condition,
            before,
            "runtime invariant broken: equipment {} condition changed after prevalidation",
            equipment.value()
        );
    }

    /// Applies one prevalidated condition change and advances the owner revision exactly once.
    pub(crate) fn apply_condition_change(
        &mut self,
        equipment: EquipmentId,
        before: Condition,
        after: Condition,
        next_revision: u64,
    ) {
        self.assert_condition_change_available(equipment, before, next_revision);
        let record = self
            .records
            .get_mut(&equipment)
            .unwrap_or_else(|| unreachable!("equipment condition change was prechecked"));
        record.condition = after;
        self.revision = next_revision;
    }

    pub(crate) fn assert_operation_condition_outcomes_available(
        &self,
        expected_revision: u64,
        next_revision: u64,
        outcomes: &[EquipmentOperationConditionOutcome],
    ) {
        assert!(!outcomes.is_empty(), "empty equipment outcome batch");
        assert_eq!(
            self.revision, expected_revision,
            "runtime invariant broken: equipment revision changed after completion precheck"
        );
        assert_eq!(
            expected_revision.checked_add(1),
            Some(next_revision),
            "completed equipment outcome batch must advance revision exactly once"
        );

        let mut seen_equipment = BTreeSet::new();
        for outcome in outcomes {
            assert!(
                seen_equipment.insert(outcome.equipment),
                "completed equipment outcome batch contains duplicate equipment {}",
                outcome.equipment.value()
            );
            let record = self.records.get(&outcome.equipment).unwrap_or_else(|| {
                panic!(
                    "runtime invariant broken: completed operation references missing equipment {}",
                    outcome.equipment.value()
                )
            });
            assert_eq!(
                record.condition,
                outcome.before,
                "runtime invariant broken: equipment {} condition changed during its occupied operation",
                outcome.equipment.value()
            );
            assert!(
                outcome.after <= outcome.before,
                "completed production operation cannot improve equipment condition"
            );
        }
    }

    /// Applies a validated simultaneous condition-outcome batch under one owner revision.
    pub(crate) fn apply_operation_condition_outcomes(
        &mut self,
        expected_revision: u64,
        next_revision: u64,
        outcomes: &[EquipmentOperationConditionOutcome],
    ) {
        self.assert_operation_condition_outcomes_available(
            expected_revision,
            next_revision,
            outcomes,
        );
        for outcome in outcomes {
            let record = match self.records.get_mut(&outcome.equipment) {
                Some(record) => record,
                None => panic!(
                    "runtime invariant broken: prechecked equipment {} disappeared during batch apply",
                    outcome.equipment.value()
                ),
            };
            record.condition = outcome.after;
        }
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
