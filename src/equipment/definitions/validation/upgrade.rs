//! Validates additive equipment-upgrade ancestry, embodiment, and preserved capability semantics.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use crate::capability::{CapabilityImprovement, CapabilityRegistry};
use crate::core::quantity::Mass;
use crate::equipment::resolve_equipment_capability;
use crate::maintenance::Condition;
use crate::material::MaterialRegistry;

use super::super::{EquipmentDefinition, EquipmentRegistry};

pub(super) fn validate_equipment_upgrade_references(
    registry: &EquipmentRegistry,
    target: &EquipmentDefinition,
    capabilities: &CapabilityRegistry,
    materials: &MaterialRegistry,
) {
    let Some(upgrade) = target.upgrade_profile() else {
        return;
    };
    let base = registry
        .definitions
        .get(&upgrade.from())
        .unwrap_or_else(|| {
            panic!(
                "equipment definition {} upgrade references missing base definition {}",
                target.id().value(),
                upgrade.from().value()
            )
        });
    validate_equipment_upgrade_capabilities(base, target, capabilities);
    assert!(
        upgrade
            .additions()
            .validate_infrastructure_references(materials)
            .is_ok(),
        "equipment definition {} upgrade additions must use existing consolidated solid commodities",
        target.id().value()
    );
    let base_assembly = base.assembly_profile().unwrap_or_else(|| {
        panic!(
            "equipment definition {} upgrade base {} has no material assembly profile",
            target.id().value(),
            base.id().value()
        )
    });
    let target_assembly = target.assembly_profile().unwrap_or_else(|| {
        panic!(
            "equipment definition {} has an upgrade profile but no material assembly profile",
            target.id().value()
        )
    });
    let expected_mass = base
        .mass()
        .checked_add(upgrade.additions().input_mass())
        .unwrap_or_else(|| {
            panic!(
                "equipment definition {} upgrade mass overflows",
                target.id().value()
            )
        });
    assert_eq!(
        target.mass(),
        expected_mass,
        "equipment definition {} upgrade mass must equal base mass plus additive material",
        target.id().value()
    );

    let mut expected_inputs = BTreeMap::new();
    for input in base_assembly
        .inputs()
        .iter()
        .chain(upgrade.additions().inputs())
    {
        let previous = expected_inputs
            .get(&input.commodity())
            .copied()
            .unwrap_or(Mass::ZERO);
        let combined = previous.checked_add(input.mass()).unwrap_or_else(|| {
            panic!(
                "equipment definition {} upgrade material quantity overflows for commodity {}",
                target.id().value(),
                input.commodity().value()
            )
        });
        expected_inputs.insert(input.commodity(), combined);
    }
    assert_eq!(
        expected_inputs.len(),
        target_assembly.inputs().len(),
        "equipment definition {} upgrade target assembly has extra or missing commodities",
        target.id().value()
    );
    for input in target_assembly.inputs() {
        assert_eq!(
            expected_inputs.get(&input.commodity()).copied(),
            Some(input.mass()),
            "equipment definition {} upgrade target assembly disagrees with base plus additive material for commodity {}",
            target.id().value(),
            input.commodity().value()
        );
    }
}

#[cfg(test)]
#[path = "upgrade_tests.rs"]
mod tests;

fn validate_equipment_upgrade_capabilities(
    base: &EquipmentDefinition,
    target: &EquipmentDefinition,
    capabilities: &CapabilityRegistry,
) {
    let first_productive_condition = Condition::new(1)
        .unwrap_or_else(|error| panic!("first productive equipment condition is invalid: {error}"));
    for (capability, _) in base.capabilities().entries() {
        let improvement = capabilities
            .get_capability(capability)
            .unwrap_or_else(|| {
                panic!(
                    "equipment definition {} upgrade base references missing capability {}",
                    base.id().value(),
                    capability.value()
                )
            })
            .improvement()
            .unwrap_or_else(|| {
                panic!(
                    "equipment definition {} additive upgrade from {} inherits capability {} without authored improvement direction",
                    target.id().value(),
                    base.id().value(),
                    capability.value()
                )
            });
        assert!(
            target.capabilities().get_capability(capability).is_some(),
            "equipment definition {} additive upgrade from {} drops capability {}",
            target.id().value(),
            base.id().value(),
            capability.value()
        );

        let mut conditions = BTreeSet::from([first_productive_condition, Condition::PRISTINE]);
        for definition in [base, target] {
            if let Some(curve) = definition.get_capability_condition_curve(capability) {
                conditions.extend(
                    curve
                        .points()
                        .iter()
                        .map(|point| point.condition())
                        .filter(|condition| *condition > Condition::FAILED),
                );
            }
        }

        for condition in conditions {
            let base_value = resolve_equipment_capability(base, condition, capability)
                .unwrap_or_else(|| {
                    panic!(
                        "equipment definition {} lost base capability {} while validating upgrade semantics",
                        base.id().value(),
                        capability.value()
                    )
                });
            let target_value = resolve_equipment_capability(target, condition, capability)
                .unwrap_or_else(|| {
                    panic!(
                        "equipment definition {} additive upgrade from {} cannot resolve inherited capability {} at {} ppm condition",
                        target.id().value(),
                        base.id().value(),
                        capability.value(),
                        condition.parts_per_million()
                    )
                });
            let ordering = target_value.compare(base_value).unwrap_or_else(|| {
                panic!(
                    "equipment definition {} additive upgrade from {} changes physical value kind for capability {}",
                    target.id().value(),
                    base.id().value(),
                    capability.value()
                )
            });
            let regressed = match improvement {
                CapabilityImprovement::Higher => ordering == Ordering::Less,
                CapabilityImprovement::Lower => ordering == Ordering::Greater,
            };
            assert!(
                !regressed,
                "equipment definition {} additive upgrade from {} regresses capability {} at {} ppm condition",
                target.id().value(),
                base.id().value(),
                capability.value(),
                condition.parts_per_million()
            );
        }
    }
}

pub(super) fn validate_equipment_upgrade_ancestry(registry: &EquipmentRegistry) {
    for definition in registry.definitions.values() {
        let mut visited = BTreeSet::new();
        let mut current = definition;
        loop {
            assert!(
                visited.insert(current.id()),
                "equipment upgrade ancestry contains a cycle at definition {}",
                current.id().value()
            );
            let Some(upgrade) = current.upgrade_profile() else {
                break;
            };
            current = registry
                .definitions
                .get(&upgrade.from())
                .unwrap_or_else(|| {
                    panic!(
                        "equipment definition {} upgrade references missing base definition {}",
                        current.id().value(),
                        upgrade.from().value()
                    )
                });
        }
    }
}
