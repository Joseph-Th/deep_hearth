//! Additive equipment-upgrade registry contracts.

use crate::capability::{
    CapabilityDefinition, CapabilityId, CapabilityImprovement, CapabilityProfile,
    CapabilityRegistry, CapabilityValue, CapabilityValueKind,
};
use crate::content::{
    EQUIPMENT_STONE_PICK, FORM_HANDLE, FORM_SCRAP, FORM_TOOL, MATERIAL_STONE, MATERIAL_WOOD,
    build_registries,
};
use crate::core::quantity::{Energy, Mass, MassFlow, Volume};
use crate::core::time::TickSpan;
use crate::equipment::{
    CapabilityConditionCurve, CapabilityConditionPoint, EquipmentDefinition, EquipmentDefinitionId,
    EquipmentMaintenanceProfile, EquipmentRegistry, EquipmentUpgradeProfile,
};
use crate::maintenance::{Condition, MaintenanceThresholds};
use crate::material::{CommodityKey, MaterialAssemblyProfile, MaterialInputSpec};
use crate::registry::Registries;
use crate::survival::SurvivalExertion;

fn fixture_capability() -> (Registries, CapabilityId) {
    let registries = build_registries();
    let capability = registries
        .equipment()
        .get_equipment(EQUIPMENT_STONE_PICK)
        .unwrap_or_else(|| panic!("built-in stone pick disappeared"))
        .capabilities()
        .entries()
        .find_map(|(capability, value)| {
            matches!(value, CapabilityValue::MassFlow(_)).then_some(capability)
        })
        .unwrap_or_else(|| panic!("stone pick lost its mass-flow capability"));
    (registries, capability)
}

fn active_exertion() -> SurvivalExertion {
    SurvivalExertion::new(Energy::from_nanojoules(1), Volume::ZERO)
}

fn component_maintenance(
    replacement: CommodityKey,
    spent: CommodityKey,
) -> EquipmentMaintenanceProfile {
    EquipmentMaintenanceProfile::new_component_replacement(
        replacement,
        Mass::from_milligrams(1),
        spent,
        Condition::PRISTINE,
        TickSpan::new(1),
        active_exertion(),
    )
}

fn two_component_assembly() -> MaterialAssemblyProfile {
    MaterialAssemblyProfile::new(vec![
        MaterialInputSpec::pure(
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            Mass::from_milligrams(1),
        ),
        MaterialInputSpec::pure(
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            Mass::from_milligrams(1),
        ),
    ])
}

fn simple_definition(
    id: EquipmentDefinitionId,
    capability: CapabilityId,
    flow_milligrams_per_second: u64,
    mass_milligrams: u64,
) -> EquipmentDefinition {
    EquipmentDefinition::new(
        id,
        "ordered upgrade capability fixture",
        Mass::from_milligrams(mass_milligrams),
        CapabilityProfile::new([(
            capability,
            CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(
                flow_milligrams_per_second,
            )),
        )])
        .unwrap_or_else(|error| panic!("ordered capability profile failed: {error}")),
        thresholds(),
    )
}

fn thresholds() -> MaintenanceThresholds {
    MaintenanceThresholds::new(
        Condition::new(600_000)
            .unwrap_or_else(|error| panic!("warning condition fixture failed: {error}")),
        Condition::new(250_000)
            .unwrap_or_else(|error| panic!("critical condition fixture failed: {error}")),
    )
    .unwrap_or_else(|error| panic!("maintenance threshold fixture failed: {error}"))
}

fn assembly(milligrams: u64) -> MaterialAssemblyProfile {
    MaterialAssemblyProfile::new(vec![MaterialInputSpec::pure(
        CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
        Mass::from_milligrams(milligrams),
    )])
}

fn base_definition(id: EquipmentDefinitionId, capability: CapabilityId) -> EquipmentDefinition {
    EquipmentDefinition::new_with_capability_condition_curves(
        id,
        "upgrade capability base fixture",
        Mass::from_milligrams(1),
        CapabilityProfile::new([(
            capability,
            CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(100)),
        )])
        .unwrap_or_else(|error| panic!("base capability profile failed: {error}")),
        thresholds(),
        vec![CapabilityConditionCurve::new(
            capability,
            vec![
                CapabilityConditionPoint::new(
                    Condition::FAILED,
                    CapabilityValue::MassFlow(MassFlow::ZERO),
                ),
                CapabilityConditionPoint::new(
                    Condition::new(500_000)
                        .unwrap_or_else(|error| panic!("base curve condition failed: {error}")),
                    CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(50)),
                ),
            ],
        )],
    )
    .with_assembly_profile(assembly(1))
}

fn target_assembly_upgrade(
    target: EquipmentDefinition,
    base: EquipmentDefinitionId,
) -> EquipmentDefinition {
    target
        .with_assembly_profile(assembly(2))
        .with_upgrade_profile(EquipmentUpgradeProfile::new(base, assembly(1)))
}

fn assert_invalid_upgrade(
    registries: &Registries,
    base: EquipmentDefinition,
    target: EquipmentDefinition,
) {
    let registry = EquipmentRegistry::new([base, target]);
    assert!(
        std::panic::catch_unwind(|| {
            registry.validate_references(registries.capabilities(), registries.materials());
        })
        .is_err()
    );
}

#[test]
fn additive_upgrade_cannot_drop_inherited_capability() {
    let (registries, capability) = fixture_capability();
    let base_id = EquipmentDefinitionId::new(810_019);
    let target_id = EquipmentDefinitionId::new(810_020);
    let base = base_definition(base_id, capability);
    let target = target_assembly_upgrade(
        EquipmentDefinition::new(
            target_id,
            "upgrade capability dropping target fixture",
            Mass::from_milligrams(2),
            CapabilityProfile::default(),
            thresholds(),
        ),
        base_id,
    );

    assert_invalid_upgrade(&registries, base, target);
}

#[test]
fn additive_upgrade_cannot_move_preserved_wear_to_a_different_service_component() {
    let (registries, capability) = fixture_capability();
    let base_id = EquipmentDefinitionId::new(810_029);
    let target_id = EquipmentDefinitionId::new(810_030);
    let base =
        base_definition(base_id, capability).with_maintenance_profile(component_maintenance(
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            CommodityKey::new(MATERIAL_STONE, FORM_SCRAP),
        ));
    let target = simple_definition(target_id, capability, 120, 2)
        .with_assembly_profile(two_component_assembly())
        .with_maintenance_profile(component_maintenance(
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            CommodityKey::new(MATERIAL_WOOD, FORM_SCRAP),
        ))
        .with_upgrade_profile(EquipmentUpgradeProfile::new(
            base_id,
            MaterialAssemblyProfile::new(vec![MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(1),
            )]),
        ));

    assert_invalid_upgrade(&registries, base, target);
}

#[test]
fn additive_upgrade_respects_lower_is_better_capability_direction() {
    let registries = build_registries();
    let capability = CapabilityId::new(810_023);
    let mut capabilities = CapabilityRegistry::new();
    capabilities.register_capability(CapabilityDefinition::new_with_improvement(
        capability,
        "ordered lower-is-better fixture",
        CapabilityValueKind::MassFlow,
        CapabilityImprovement::Lower,
    ));
    let base_id = EquipmentDefinitionId::new(810_024);
    let target_id = EquipmentDefinitionId::new(810_025);
    let base = simple_definition(base_id, capability, 100, 1).with_assembly_profile(assembly(1));
    let target = target_assembly_upgrade(simple_definition(target_id, capability, 80, 2), base_id);
    let registry = EquipmentRegistry::new([base, target]);

    registry.validate_references(&capabilities, registries.materials());
}

#[test]
fn additive_upgrade_requires_authored_capability_improvement_direction() {
    let registries = build_registries();
    let capability = CapabilityId::new(810_026);
    let mut capabilities = CapabilityRegistry::new();
    capabilities.register_capability(CapabilityDefinition::new(
        capability,
        "unordered upgrade capability fixture",
        CapabilityValueKind::MassFlow,
    ));
    let base_id = EquipmentDefinitionId::new(810_027);
    let target_id = EquipmentDefinitionId::new(810_028);
    let base = simple_definition(base_id, capability, 100, 1).with_assembly_profile(assembly(1));
    let target = target_assembly_upgrade(simple_definition(target_id, capability, 120, 2), base_id);
    let registry = EquipmentRegistry::new([base, target]);

    assert!(
        std::panic::catch_unwind(|| {
            registry.validate_references(&capabilities, registries.materials());
        })
        .is_err(),
        "additive upgrade semantics require an authored capability improvement direction"
    );
}

#[test]
fn additive_upgrade_cannot_regress_inherited_capability_at_preserved_condition() {
    let (registries, capability) = fixture_capability();
    let base_id = EquipmentDefinitionId::new(810_021);
    let target_id = EquipmentDefinitionId::new(810_022);
    let base = base_definition(base_id, capability);
    let target = target_assembly_upgrade(
        EquipmentDefinition::new_with_capability_condition_curves(
            target_id,
            "upgrade capability regressing target fixture",
            Mass::from_milligrams(2),
            CapabilityProfile::new([(
                capability,
                CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(120)),
            )])
            .unwrap_or_else(|error| panic!("target capability profile failed: {error}")),
            thresholds(),
            vec![CapabilityConditionCurve::new(
                capability,
                vec![
                    CapabilityConditionPoint::new(
                        Condition::FAILED,
                        CapabilityValue::MassFlow(MassFlow::ZERO),
                    ),
                    CapabilityConditionPoint::new(
                        Condition::new(500_000).unwrap_or_else(|error| {
                            panic!("target curve condition failed: {error}")
                        }),
                        CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(40)),
                    ),
                ],
            )],
        ),
        base_id,
    );

    assert_invalid_upgrade(&registries, base, target);
}
