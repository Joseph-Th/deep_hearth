//! Condition, wear, and material-backed maintenance mutations for the equipment owner.

use std::collections::BTreeSet;

use crate::core::quantity::Mass;
use crate::maintenance::Condition;

use super::{
    EquipmentComponentMaintenanceMutation, EquipmentId, EquipmentMaintenanceAdmission,
    EquipmentOperationConditionOutcome, EquipmentState,
};

impl EquipmentState {
    pub(in crate::equipment) fn assert_component_maintenance_available(
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
        assert_eq!(
            mutation.admission.condition_before(),
            mutation.condition_before
        );
        assert_eq!(
            mutation.admission.equipment_revision(),
            next_revision,
            "component maintenance receipt must bind the admission equipment revision"
        );
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

    pub(in crate::equipment) fn assert_maintenance_admission_available(
        &self,
        equipment: EquipmentId,
        admission: EquipmentMaintenanceAdmission,
        expected_revision: u64,
        next_revision: u64,
    ) {
        assert_eq!(self.revision, expected_revision);
        assert_eq!(expected_revision.checked_add(1), Some(next_revision));
        assert_eq!(
            admission.equipment_revision(),
            next_revision,
            "maintenance receipt must bind the admission equipment revision"
        );
        let record = self.records.get(&equipment).unwrap_or_else(|| {
            panic!(
                "runtime invariant broken: equipment {} disappeared before maintenance admission",
                equipment.value()
            )
        });
        assert_eq!(record.condition, admission.condition_before());
    }

    /// Advances equipment freshness when service begins without granting the future condition gain.
    pub(in crate::equipment) fn apply_maintenance_admission(
        &mut self,
        equipment: EquipmentId,
        admission: EquipmentMaintenanceAdmission,
        expected_revision: u64,
        next_revision: u64,
    ) {
        self.assert_maintenance_admission_available(
            equipment,
            admission,
            expected_revision,
            next_revision,
        );
        let record = self
            .records
            .get_mut(&equipment)
            .unwrap_or_else(|| unreachable!("maintenance-admission equipment was prechecked"));
        record.last_maintenance_admission = Some(admission);
        self.revision = next_revision;
    }

    /// Exchanges every trace belonging to one authored component for exact fresh traces while
    /// preserving equipment identity, all unrelated embodied matter, and total embodied mass.
    pub(in crate::equipment) fn apply_component_maintenance(
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
        record.last_maintenance_admission = Some(mutation.admission);
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
}
