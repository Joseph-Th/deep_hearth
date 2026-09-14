//! Contract tests for equipment definitions.

use super::*;
use crate::capability::{
    CapabilityDefinition, CapabilityImprovement, CapabilityRegistry, CapabilityValue,
    CapabilityValueKind,
};
use crate::content::{FORM_SCRAP, FORM_TOOL, MATERIAL_STONE};
use crate::core::quantity::{Energy, Volume};
use crate::core::time::TickSpan;
use crate::maintenance::Condition;
use crate::material::{CommodityKey, MaterialInputSpec};
use crate::survival::SurvivalExertion;

fn active_exertion() -> SurvivalExertion {
    SurvivalExertion::new(Energy::from_nanojoules(1), Volume::ZERO)
}

fn definition_with_condition_curve(
    id: EquipmentDefinitionId,
    capability: CapabilityId,
    nominal: u64,
    failed: u64,
) -> EquipmentDefinition {
    EquipmentDefinition::new_with_capability_condition_curves(
        id,
        "condition curve validation fixture",
        Mass::from_milligrams(1),
        CapabilityProfile::new([(
            capability,
            CapabilityValue::Mass(Mass::from_milligrams(nominal)),
        )])
        .unwrap_or_else(|error| panic!("condition curve profile fixture failed: {error}")),
        MaintenanceThresholds::new(
            Condition::new(600_000)
                .unwrap_or_else(|error| panic!("warning condition fixture failed: {error}")),
            Condition::new(250_000)
                .unwrap_or_else(|error| panic!("critical condition fixture failed: {error}")),
        )
        .unwrap_or_else(|error| panic!("condition curve threshold fixture failed: {error}")),
        vec![CapabilityConditionCurve::new(
            capability,
            vec![CapabilityConditionPoint::new(
                Condition::FAILED,
                CapabilityValue::Mass(Mass::from_milligrams(failed)),
            )],
        )],
    )
}

fn condition_curve_registry_rejects(
    capability: CapabilityDefinition,
    definition: EquipmentDefinition,
) {
    let registries = crate::content::build_registries();
    let mut capabilities = CapabilityRegistry::new();
    capabilities.register_capability(capability);
    let registry = EquipmentRegistry::new([definition]);
    assert!(
        std::panic::catch_unwind(|| {
            registry.validate_references(&capabilities, registries.materials());
        })
        .is_err()
    );
}

#[test]
fn condition_curve_requires_authored_capability_improvement_direction() {
    let capability = CapabilityId::new(810_030);
    condition_curve_registry_rejects(
        CapabilityDefinition::new(
            capability,
            "unordered condition-sensitive capability",
            CapabilityValueKind::Mass,
        ),
        definition_with_condition_curve(EquipmentDefinitionId::new(810_031), capability, 100, 50),
    );
}

#[test]
fn condition_curve_cannot_make_higher_is_better_capability_improve_with_damage() {
    let capability = CapabilityId::new(810_032);
    condition_curve_registry_rejects(
        CapabilityDefinition::new_with_improvement(
            capability,
            "higher is better condition-sensitive capability",
            CapabilityValueKind::Mass,
            CapabilityImprovement::Higher,
        ),
        definition_with_condition_curve(EquipmentDefinitionId::new(810_033), capability, 100, 150),
    );
}

#[test]
fn condition_curve_cannot_make_lower_is_better_capability_improve_with_damage() {
    let capability = CapabilityId::new(810_034);
    condition_curve_registry_rejects(
        CapabilityDefinition::new_with_improvement(
            capability,
            "lower is better condition-sensitive capability",
            CapabilityValueKind::Mass,
            CapabilityImprovement::Lower,
        ),
        definition_with_condition_curve(EquipmentDefinitionId::new(810_035), capability, 100, 50),
    );
}

#[test]
fn condition_curve_accepts_lower_is_better_capability_that_worsens_with_damage() {
    let registries = crate::content::build_registries();
    let capability = CapabilityId::new(810_036);
    let mut capabilities = CapabilityRegistry::new();
    capabilities.register_capability(CapabilityDefinition::new_with_improvement(
        capability,
        "lower is better degrading capability",
        CapabilityValueKind::Mass,
        CapabilityImprovement::Lower,
    ));
    let registry = EquipmentRegistry::new([definition_with_condition_curve(
        EquipmentDefinitionId::new(810_037),
        capability,
        100,
        150,
    )]);

    registry.validate_references(&capabilities, registries.materials());
}

fn assembly_profile() -> MaterialAssemblyProfile {
    MaterialAssemblyProfile::new(vec![MaterialInputSpec::pure(
        CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
        Mass::from_milligrams(1),
    )])
}

#[test]
fn component_maintenance_requires_the_complete_component_at_any_wear_level() {
    let profile = EquipmentMaintenanceProfile::new_component_replacement(
        CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
        Mass::from_milligrams(7),
        CommodityKey::new(MATERIAL_STONE, FORM_SCRAP),
        Condition::PRISTINE,
        TickSpan::new(7),
        active_exertion(),
    );

    assert!(profile.is_component_replacement());
    assert_eq!(
        profile.required_replacement_mass(Condition::FAILED),
        Mass::from_milligrams(7)
    );
    assert_eq!(
        profile.required_replacement_mass(
            Condition::new(999_999)
                .unwrap_or_else(|error| panic!("worn component condition failed: {error}"))
        ),
        Mass::from_milligrams(7),
        "component service must not turn a partial component into a free condition reset"
    );
    assert_eq!(
        profile.required_replacement_mass(Condition::PRISTINE),
        Mass::ZERO
    );
    assert_eq!(
        profile.required_service_duration(Condition::FAILED),
        TickSpan::new(7)
    );
    assert_eq!(
        profile.required_service_duration(
            Condition::new(999_999)
                .unwrap_or_else(|error| panic!("worn component condition failed: {error}"))
        ),
        TickSpan::new(7),
        "component replacement duration must remain indivisible with the whole component"
    );
    assert_eq!(
        profile.required_service_duration(Condition::PRISTINE),
        TickSpan::ZERO
    );
}

#[test]
fn component_maintenance_cannot_leave_unowned_residual_wear_after_full_replacement() {
    let result = std::panic::catch_unwind(|| {
        EquipmentMaintenanceProfile::new_component_replacement(
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            Mass::from_milligrams(7),
            CommodityKey::new(MATERIAL_STONE, FORM_SCRAP),
            Condition::new(900_000)
                .unwrap_or_else(|error| panic!("residual-wear condition fixture failed: {error}")),
            TickSpan::new(7),
            active_exertion(),
        )
    });

    assert!(result.is_err());
}

#[test]
fn maintenance_replacement_mass_tracks_condition_restored_and_rounds_positive_repairs_up() {
    let profile = EquipmentMaintenanceProfile::new(
        CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
        Mass::from_milligrams(7),
        CommodityKey::new(MATERIAL_STONE, FORM_SCRAP),
        Condition::new(700_000)
            .unwrap_or_else(|error| panic!("maintenance target fixture failed: {error}")),
        TickSpan::new(7),
        active_exertion(),
    );

    assert_eq!(
        profile.required_replacement_mass(Condition::FAILED),
        Mass::from_milligrams(7),
        "failed-to-target service must consume the authored full-service mass"
    );
    assert_eq!(
        profile.required_replacement_mass(
            Condition::new(500_000)
                .unwrap_or_else(|error| panic!("partial condition fixture failed: {error}"))
        ),
        Mass::from_milligrams(2),
        "partial service must scale with the condition actually restored"
    );
    assert_eq!(
        profile.required_replacement_mass(
            Condition::new(699_999)
                .unwrap_or_else(|error| panic!("near-target condition fixture failed: {error}"))
        ),
        Mass::from_milligrams(1),
        "positive repair must never round down to free maintenance"
    );
    assert_eq!(
        profile.required_replacement_mass(
            Condition::new(700_000)
                .unwrap_or_else(|error| panic!("target condition fixture failed: {error}"))
        ),
        Mass::ZERO,
        "service at the target must require no replacement stock"
    );
    assert_eq!(
        profile.required_service_duration(Condition::FAILED),
        TickSpan::new(7),
        "failed-to-target service must require the authored full-service duration"
    );
    assert_eq!(
        profile.required_service_duration(
            Condition::new(500_000)
                .unwrap_or_else(|error| panic!("partial condition fixture failed: {error}"))
        ),
        TickSpan::new(2),
        "partial service duration must scale with the condition actually restored"
    );
    assert_eq!(
        profile.required_service_duration(
            Condition::new(699_999)
                .unwrap_or_else(|error| panic!("near-target condition fixture failed: {error}"))
        ),
        TickSpan::new(1),
        "positive repair must never round down to zero work"
    );
    assert_eq!(
        profile.required_service_duration(
            Condition::new(700_000)
                .unwrap_or_else(|error| panic!("target condition fixture failed: {error}"))
        ),
        TickSpan::ZERO
    );
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

#[test]
fn authored_acquisition_edge_classification_follows_assembly_and_upgrade_fields() {
    let unavailable = basic_definition(EquipmentDefinitionId::new(810_011));
    let assembled = basic_definition(EquipmentDefinitionId::new(810_012))
        .with_assembly_profile(assembly_profile());
    let upgraded = basic_definition(EquipmentDefinitionId::new(810_013)).with_upgrade_profile(
        EquipmentUpgradeProfile::new(EquipmentDefinitionId::new(810_014), assembly_profile()),
    );

    assert!(!unavailable.has_authored_acquisition_edge());
    assert!(assembled.has_authored_acquisition_edge());
    assert!(upgraded.has_authored_acquisition_edge());
}

fn maintenance_registry(spent: CommodityKey) -> EquipmentRegistry {
    EquipmentRegistry::new([basic_definition(EquipmentDefinitionId::new(810_003))
        .with_maintenance_profile(EquipmentMaintenanceProfile::new(
            CommodityKey::new(crate::content::MATERIAL_COPPER, crate::content::FORM_INGOT),
            Mass::from_milligrams(1),
            spent,
            Condition::new(900_000)
                .unwrap_or_else(|error| panic!("maintenance target fixture failed: {error}")),
            TickSpan::new(1),
            active_exertion(),
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
            TickSpan::new(1),
            active_exertion(),
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
    assert!(duplicate_upgrade.is_err());
}

#[test]
fn equipment_registry_rejects_cyclic_upgrade_ancestry() {
    let registries = crate::content::build_registries();
    let first_id = EquipmentDefinitionId::new(810_017);
    let second_id = EquipmentDefinitionId::new(810_018);
    let first = basic_definition(first_id)
        .with_upgrade_profile(EquipmentUpgradeProfile::new(second_id, assembly_profile()));
    let second = basic_definition(second_id)
        .with_upgrade_profile(EquipmentUpgradeProfile::new(first_id, assembly_profile()));
    let invalid = EquipmentRegistry::new([first, second]);

    assert!(
        std::panic::catch_unwind(|| {
            invalid.validate_references(registries.capabilities(), registries.materials());
        })
        .is_err()
    );
}

#[test]
fn equipment_definition_rejects_aggregate_maintenance_on_exact_assembled_matter() {
    let maintenance = || {
        EquipmentMaintenanceProfile::new(
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            Mass::from_milligrams(1),
            CommodityKey::new(MATERIAL_STONE, FORM_SCRAP),
            Condition::new(900_000)
                .unwrap_or_else(|error| panic!("maintenance target fixture failed: {error}")),
            TickSpan::new(1),
            active_exertion(),
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
fn equipment_registry_accepts_component_replacement_that_matches_exact_assembly_input() {
    let registries = crate::content::build_registries();
    let definition = basic_definition(EquipmentDefinitionId::new(810_015))
        .with_assembly_profile(assembly_profile())
        .with_maintenance_profile(EquipmentMaintenanceProfile::new_component_replacement(
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            Mass::from_milligrams(1),
            CommodityKey::new(MATERIAL_STONE, FORM_SCRAP),
            Condition::PRISTINE,
            TickSpan::new(1),
            active_exertion(),
        ));
    let registry = EquipmentRegistry::new([definition]);

    registry.validate_references(registries.capabilities(), registries.materials());
}

#[test]
fn equipment_registry_rejects_component_replacement_mass_that_is_not_whole_component() {
    let registries = crate::content::build_registries();
    let definition = basic_definition(EquipmentDefinitionId::new(810_016))
        .with_assembly_profile(assembly_profile())
        .with_maintenance_profile(EquipmentMaintenanceProfile::new_component_replacement(
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            Mass::from_milligrams(2),
            CommodityKey::new(MATERIAL_STONE, FORM_SCRAP),
            Condition::PRISTINE,
            TickSpan::new(1),
            active_exertion(),
        ));
    let registry = EquipmentRegistry::new([definition]);

    assert!(
        std::panic::catch_unwind(|| {
            registry.validate_references(registries.capabilities(), registries.materials());
        })
        .is_err()
    );
}

#[test]
fn equipment_maintenance_rejects_resting_exertion() {
    let result = std::panic::catch_unwind(|| {
        EquipmentMaintenanceProfile::new(
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            Mass::from_milligrams(1),
            CommodityKey::new(MATERIAL_STONE, FORM_SCRAP),
            Condition::PRISTINE,
            TickSpan::new(1),
            SurvivalExertion::REST,
        )
    });

    assert!(result.is_err());
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
