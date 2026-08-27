//! Tests for the sibling definitions module; isolated so test-only edits do not invalidate production builds.

use super::*;
use crate::content::{FORM_SCRAP, FORM_TOOL, MATERIAL_STONE};
use crate::material::MaterialInputSpec;

fn assembly_profile() -> MaterialAssemblyProfile {
    MaterialAssemblyProfile::new(vec![MaterialInputSpec::new(
        CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
        Mass::from_milligrams(1),
    )])
}

fn basic_definition(id: EquipmentDefinitionId) -> EquipmentDefinition {
    let thresholds = MaintenanceThresholds::new(
        Condition::new(600_000)
            .unwrap_or_else(|error| panic!("warning condition fixture failed: {error}")),
        Condition::new(250_000)
            .unwrap_or_else(|error| panic!("critical condition fixture failed: {error}")),
    )
    .unwrap_or_else(|error| panic!("maintenance threshold fixture failed: {error}"));
    let capabilities =
        CapabilityProfile::new(std::iter::empty::<(CapabilityId, CapabilityValue)>())
            .unwrap_or_else(|error| panic!("empty capability profile fixture failed: {error}"));
    EquipmentDefinition::new(
        id,
        "equipment definition fixture",
        Mass::from_milligrams(1),
        capabilities,
        thresholds,
    )
}

fn maintenance_registry(spent: CommodityKey) -> EquipmentRegistry {
    EquipmentRegistry::new([basic_definition(EquipmentDefinitionId::new(810_003))
        .with_maintenance_profile(EquipmentMaintenanceProfile::new(
            CommodityKey::new(crate::content::MATERIAL_COPPER, crate::content::FORM_INGOT),
            Mass::from_milligrams(1),
            spent,
            Condition::new(900_000)
                .unwrap_or_else(|error| panic!("maintenance target fixture failed: {error}")),
        ))])
}

fn assert_invalid_maintenance_reform(spent: CommodityKey) {
    let registries = crate::content::build_registries();
    let registry = maintenance_registry(spent);
    let result = std::panic::catch_unwind(|| {
        registry.validate_references(registries.capabilities(), registries.materials());
    });
    assert!(result.is_err());
}

#[test]
fn continuous_condition_curve_rejects_presence_capability() {
    let capability = CapabilityId::new(810_001);
    let result = std::panic::catch_unwind(|| {
        CapabilityConditionCurve::new(
            capability,
            vec![CapabilityConditionPoint::new(
                Condition::FAILED,
                CapabilityValue::Present,
            )],
        )
    });

    assert!(result.is_err());
}

#[test]
fn maintenance_registry_rejects_phase_change_without_thermal_process() {
    assert_invalid_maintenance_reform(CommodityKey::new(
        crate::content::MATERIAL_COPPER,
        crate::content::FORM_MOLTEN,
    ));
}

#[test]
fn maintenance_registry_rejects_particle_state_change_without_transform_process() {
    assert_invalid_maintenance_reform(CommodityKey::new(
        crate::content::MATERIAL_COPPER,
        crate::content::FORM_CRUSHED,
    ));
}

#[test]
fn equipment_definition_rejects_duplicate_authoritative_profiles() {
    let maintenance = || {
        EquipmentMaintenanceProfile::new(
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            Mass::from_milligrams(1),
            CommodityKey::new(MATERIAL_STONE, FORM_SCRAP),
            Condition::new(900_000)
                .unwrap_or_else(|error| panic!("maintenance target fixture failed: {error}")),
        )
    };
    let duplicate_maintenance = std::panic::catch_unwind(|| {
        basic_definition(EquipmentDefinitionId::new(810_004))
            .with_maintenance_profile(maintenance())
            .with_maintenance_profile(maintenance())
    });
    let duplicate_assembly = std::panic::catch_unwind(|| {
        basic_definition(EquipmentDefinitionId::new(810_005))
            .with_assembly_profile(assembly_profile())
            .with_assembly_profile(assembly_profile())
    });
    let duplicate_recovery = std::panic::catch_unwind(|| {
        basic_definition(EquipmentDefinitionId::new(810_006))
            .with_assembly_profile(assembly_profile())
            .with_worn_recovery_form(FORM_SCRAP)
            .with_worn_recovery_form(FORM_SCRAP)
    });
    let duplicate_upgrade = std::panic::catch_unwind(|| {
        basic_definition(EquipmentDefinitionId::new(810_007))
            .with_upgrade_profile(EquipmentUpgradeProfile::new(
                EquipmentDefinitionId::new(810_008),
                assembly_profile(),
            ))
            .with_upgrade_profile(EquipmentUpgradeProfile::new(
                EquipmentDefinitionId::new(810_008),
                assembly_profile(),
            ))
    });

    assert!(duplicate_maintenance.is_err());
    assert!(duplicate_assembly.is_err());
    assert!(duplicate_recovery.is_err());
    assert!(duplicate_upgrade.is_err());
}

#[test]
fn equipment_definition_rejects_maintenance_on_exact_assembled_matter() {
    let maintenance = || {
        EquipmentMaintenanceProfile::new(
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            Mass::from_milligrams(1),
            CommodityKey::new(MATERIAL_STONE, FORM_SCRAP),
            Condition::new(900_000)
                .unwrap_or_else(|error| panic!("maintenance target fixture failed: {error}")),
        )
    };
    let assembly_then_maintenance = std::panic::catch_unwind(|| {
        basic_definition(EquipmentDefinitionId::new(810_009))
            .with_assembly_profile(assembly_profile())
            .with_maintenance_profile(maintenance())
    });
    let maintenance_then_assembly = std::panic::catch_unwind(|| {
        basic_definition(EquipmentDefinitionId::new(810_010))
            .with_maintenance_profile(maintenance())
            .with_assembly_profile(assembly_profile())
    });

    assert!(assembly_then_maintenance.is_err());
    assert!(maintenance_then_assembly.is_err());
}

#[test]
fn condition_curve_rejects_nonmonotonic_recovery_toward_nominal_value() {
    let capability = CapabilityId::new(810_002);
    let nominal = CapabilityValue::Mass(Mass::from_milligrams(100));
    let profile = CapabilityProfile::new([(capability, nominal)])
        .unwrap_or_else(|error| panic!("capability profile fixture failed: {error}"));
    let thresholds = MaintenanceThresholds::new(
        Condition::new(600_000)
            .unwrap_or_else(|error| panic!("warning condition fixture failed: {error}")),
        Condition::new(250_000)
            .unwrap_or_else(|error| panic!("critical condition fixture failed: {error}")),
    )
    .unwrap_or_else(|error| panic!("maintenance threshold fixture failed: {error}"));
    let curve = CapabilityConditionCurve::new(
        capability,
        vec![
            CapabilityConditionPoint::new(Condition::FAILED, CapabilityValue::Mass(Mass::ZERO)),
            CapabilityConditionPoint::new(
                Condition::new(500_000)
                    .unwrap_or_else(|error| panic!("midpoint condition fixture failed: {error}")),
                CapabilityValue::Mass(Mass::from_milligrams(80)),
            ),
            CapabilityConditionPoint::new(
                Condition::new(750_000)
                    .unwrap_or_else(|error| panic!("late condition fixture failed: {error}")),
                CapabilityValue::Mass(Mass::from_milligrams(70)),
            ),
        ],
    );

    let result = std::panic::catch_unwind(|| {
        EquipmentDefinition::new_with_capability_condition_curves(
            EquipmentDefinitionId::new(810_002),
            "nonmonotonic condition fixture",
            Mass::from_milligrams(1),
            profile,
            thresholds,
            vec![curve],
        )
    });

    assert!(result.is_err());
}
