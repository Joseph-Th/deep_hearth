//! Built-in workshop equipment definitions.

use crate::capability::{CapabilityId, CapabilityProfile, CapabilityValue};
use crate::core::quantity::{Mass, MassFlow, Power, Pressure, Temperature};
use crate::equipment::{
    CapabilityConditionCurve, CapabilityConditionPoint, EquipmentDefinition, EquipmentDefinitionId,
    EquipmentMaintenanceProfile, EquipmentRegistry, EquipmentUpgradeProfile,
};
use crate::maintenance::{Condition, MaintenanceThresholds};
use crate::material::{CommodityKey, MaterialAssemblyProfile, MaterialInputSpec};

use super::capabilities::{
    CAPABILITY_COOLING_POWER, CAPABILITY_CRUSHER_BATCH, CAPABILITY_CRUSHER_FLOW,
    CAPABILITY_GRINDER_BATCH, CAPABILITY_GRINDER_FLOW, CAPABILITY_HEATING_POWER,
    CAPABILITY_MANUAL_POWER_OUTPUT, CAPABILITY_MINING_FLOW, CAPABILITY_MINING_MAX_BATCH,
    CAPABILITY_MINING_MAX_HARDNESS, CAPABILITY_SCREEN_BATCH, CAPABILITY_SCREEN_FLOW,
    CAPABILITY_THERMAL_BATCH, CAPABILITY_THERMAL_MAX_TEMPERATURE,
};
use super::materials::{
    FORM_FLYWHEEL, FORM_HANDLE, FORM_INGOT, FORM_REINFORCEMENT, FORM_SCRAP, FORM_TOOL,
    MATERIAL_COPPER, MATERIAL_STONE, MATERIAL_WOOD,
};

pub const EQUIPMENT_JAW_CRUSHER: EquipmentDefinitionId = EquipmentDefinitionId::new(1);
pub const EQUIPMENT_ELECTRIC_FURNACE: EquipmentDefinitionId = EquipmentDefinitionId::new(2);
pub const EQUIPMENT_CASTING_MOLD: EquipmentDefinitionId = EquipmentDefinitionId::new(3);
pub const EQUIPMENT_DRY_SCREEN: EquipmentDefinitionId = EquipmentDefinitionId::new(4);
pub const EQUIPMENT_GRINDING_MILL: EquipmentDefinitionId = EquipmentDefinitionId::new(5);
pub const EQUIPMENT_STONE_PICK: EquipmentDefinitionId = EquipmentDefinitionId::new(6);
pub const EQUIPMENT_STONE_HAND_CRANK: EquipmentDefinitionId = EquipmentDefinitionId::new(7);
pub const EQUIPMENT_COPPER_REINFORCED_PICK: EquipmentDefinitionId = EquipmentDefinitionId::new(8);
pub const EQUIPMENT_COPPER_REINFORCED_HAND_CRANK: EquipmentDefinitionId =
    EquipmentDefinitionId::new(9);
pub const EQUIPMENT_STONE_CRUSHER: EquipmentDefinitionId = EquipmentDefinitionId::new(10);

fn condition(parts_per_million: u32) -> Condition {
    match Condition::new(parts_per_million) {
        Ok(condition) => condition,
        Err(error) => panic!("built-in equipment condition is invalid: {error}"),
    }
}

fn workshop_maintenance() -> EquipmentMaintenanceProfile {
    EquipmentMaintenanceProfile::new(
        CommodityKey::new(MATERIAL_COPPER, FORM_INGOT),
        Mass::from_milligrams(50_000),
        CommodityKey::new(MATERIAL_COPPER, FORM_SCRAP),
        condition(900_000),
    )
}

fn thresholds() -> MaintenanceThresholds {
    match MaintenanceThresholds::new(condition(600_000), condition(250_000)) {
        Ok(thresholds) => thresholds,
        Err(error) => panic!("built-in equipment maintenance thresholds are invalid: {error}"),
    }
}

fn profile(
    entries: impl IntoIterator<Item = (CapabilityId, CapabilityValue)>,
) -> CapabilityProfile {
    match CapabilityProfile::new(entries) {
        Ok(profile) => profile,
        Err(error) => panic!("built-in equipment capability profile is invalid: {error}"),
    }
}

fn mass_flow_condition_curve(
    capability: CapabilityId,
    degraded_condition_ppm: u32,
    degraded_flow: MassFlow,
) -> CapabilityConditionCurve {
    CapabilityConditionCurve::new(
        capability,
        vec![
            CapabilityConditionPoint::new(
                Condition::FAILED,
                CapabilityValue::MassFlow(MassFlow::ZERO),
            ),
            CapabilityConditionPoint::new(
                condition(degraded_condition_ppm),
                CapabilityValue::MassFlow(degraded_flow),
            ),
        ],
    )
}

fn power_condition_curve(
    capability: CapabilityId,
    degraded_condition_ppm: u32,
    degraded_power: Power,
) -> CapabilityConditionCurve {
    CapabilityConditionCurve::new(
        capability,
        vec![
            CapabilityConditionPoint::new(Condition::FAILED, CapabilityValue::Power(Power::ZERO)),
            CapabilityConditionPoint::new(
                condition(degraded_condition_ppm),
                CapabilityValue::Power(degraded_power),
            ),
        ],
    )
}

pub(crate) fn build_equipment_registry() -> EquipmentRegistry {
    let crusher_curve = mass_flow_condition_curve(
        CAPABILITY_CRUSHER_FLOW,
        600_000,
        MassFlow::from_milligrams_per_second(2_000_000),
    );
    let stone_crusher_curve = mass_flow_condition_curve(
        CAPABILITY_CRUSHER_FLOW,
        600_000,
        MassFlow::from_milligrams_per_second(2_500),
    );
    let reinforced_hand_crank_curve = power_condition_curve(
        CAPABILITY_MANUAL_POWER_OUTPUT,
        500_000,
        Power::from_microwatts(50_000_000),
    );
    let reinforced_mining_curve = mass_flow_condition_curve(
        CAPABILITY_MINING_FLOW,
        500_000,
        MassFlow::from_milligrams_per_second(15_000),
    );
    let mining_curve = mass_flow_condition_curve(
        CAPABILITY_MINING_FLOW,
        500_000,
        MassFlow::from_milligrams_per_second(10_000),
    );
    let grinder_curve = mass_flow_condition_curve(
        CAPABILITY_GRINDER_FLOW,
        600_000,
        MassFlow::from_milligrams_per_second(1_000_000),
    );
    let screen_curve = mass_flow_condition_curve(
        CAPABILITY_SCREEN_FLOW,
        600_000,
        MassFlow::from_milligrams_per_second(4_000_000),
    );
    let hand_crank_curve = power_condition_curve(
        CAPABILITY_MANUAL_POWER_OUTPUT,
        500_000,
        Power::from_microwatts(25_000_000),
    );
    let furnace_curve = power_condition_curve(
        CAPABILITY_HEATING_POWER,
        600_000,
        Power::from_microwatts(1_000_000_000_000),
    );
    let casting_mold_curve = power_condition_curve(
        CAPABILITY_COOLING_POWER,
        600_000,
        Power::from_microwatts(500_000_000_000),
    );
    EquipmentRegistry::new([
        EquipmentDefinition::new_with_capability_condition_curves(
            EQUIPMENT_JAW_CRUSHER,
            "workshop jaw crusher",
            Mass::from_milligrams(3_600_000_000),
            profile([
                (
                    CAPABILITY_CRUSHER_FLOW,
                    CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(4_000_000)),
                ),
                (
                    CAPABILITY_CRUSHER_BATCH,
                    CapabilityValue::Mass(Mass::from_milligrams(20_000_000)),
                ),
            ]),
            thresholds(),
            vec![crusher_curve],
        )
        .with_required_structural_support()
        .with_maintenance_profile(workshop_maintenance()),
        EquipmentDefinition::new_with_capability_condition_curves(
            EQUIPMENT_ELECTRIC_FURNACE,
            "workshop electric furnace",
            Mass::from_milligrams(500_000_000),
            profile([
                (
                    CAPABILITY_HEATING_POWER,
                    CapabilityValue::Power(Power::from_microwatts(2_000_000_000_000)),
                ),
                (
                    CAPABILITY_THERMAL_MAX_TEMPERATURE,
                    CapabilityValue::Temperature(Temperature::from_millikelvin(1_500_000)),
                ),
                (
                    CAPABILITY_THERMAL_BATCH,
                    CapabilityValue::Mass(Mass::from_milligrams(20_000_000)),
                ),
            ]),
            thresholds(),
            vec![furnace_curve],
        )
        .with_required_structural_support()
        .with_maintenance_profile(workshop_maintenance()),
        EquipmentDefinition::new_with_capability_condition_curves(
            EQUIPMENT_CASTING_MOLD,
            "workshop cooled casting mold",
            Mass::from_milligrams(100_000_000),
            profile([
                (
                    CAPABILITY_COOLING_POWER,
                    CapabilityValue::Power(Power::from_microwatts(1_000_000_000_000)),
                ),
                (
                    CAPABILITY_THERMAL_MAX_TEMPERATURE,
                    CapabilityValue::Temperature(Temperature::from_millikelvin(1_600_000)),
                ),
                (
                    CAPABILITY_THERMAL_BATCH,
                    CapabilityValue::Mass(Mass::from_milligrams(20_000_000)),
                ),
            ]),
            thresholds(),
            vec![casting_mold_curve],
        )
        .with_required_structural_support()
        .with_maintenance_profile(workshop_maintenance()),
        EquipmentDefinition::new_with_capability_condition_curves(
            EQUIPMENT_DRY_SCREEN,
            "workshop dry screen",
            Mass::from_milligrams(1_200_000_000),
            profile([
                (
                    CAPABILITY_SCREEN_FLOW,
                    CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(8_000_000)),
                ),
                (
                    CAPABILITY_SCREEN_BATCH,
                    CapabilityValue::Mass(Mass::from_milligrams(20_000_000)),
                ),
            ]),
            thresholds(),
            vec![screen_curve],
        )
        .with_required_structural_support()
        .with_maintenance_profile(workshop_maintenance()),
        EquipmentDefinition::new_with_capability_condition_curves(
            EQUIPMENT_GRINDING_MILL,
            "workshop grinding mill",
            Mass::from_milligrams(2_400_000_000),
            profile([
                (
                    CAPABILITY_GRINDER_FLOW,
                    CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(2_000_000)),
                ),
                (
                    CAPABILITY_GRINDER_BATCH,
                    CapabilityValue::Mass(Mass::from_milligrams(20_000_000)),
                ),
            ]),
            thresholds(),
            vec![grinder_curve],
        )
        .with_required_structural_support()
        .with_maintenance_profile(workshop_maintenance()),
        EquipmentDefinition::new_with_capability_condition_curves(
            EQUIPMENT_STONE_PICK,
            "knapped stone pick",
            Mass::from_milligrams(1_000_000),
            profile([
                (
                    CAPABILITY_MINING_FLOW,
                    CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(20_000)),
                ),
                (
                    CAPABILITY_MINING_MAX_BATCH,
                    CapabilityValue::Mass(Mass::from_milligrams(200_000)),
                ),
                (
                    CAPABILITY_MINING_MAX_HARDNESS,
                    CapabilityValue::Pressure(Pressure::from_pascals(500_000_000)),
                ),
            ]),
            thresholds(),
            vec![mining_curve],
        )
        .with_assembly_profile(MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::new(
                CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
                Mass::from_milligrams(800_000),
            ),
            MaterialInputSpec::new(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(200_000),
            ),
        ]))
        .with_worn_recovery_form(FORM_SCRAP),
        EquipmentDefinition::new_with_capability_condition_curves(
            EQUIPMENT_STONE_HAND_CRANK,
            "stone hand crank",
            Mass::from_milligrams(1_100_000),
            profile([(
                CAPABILITY_MANUAL_POWER_OUTPUT,
                CapabilityValue::Power(Power::from_microwatts(50_000_000)),
            )]),
            thresholds(),
            vec![hand_crank_curve],
        )
        .with_assembly_profile(MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::new(
                CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
                Mass::from_milligrams(900_000),
            ),
            MaterialInputSpec::new(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(200_000),
            ),
        ]))
        .with_worn_recovery_form(FORM_SCRAP),
        EquipmentDefinition::new_with_capability_condition_curves(
            EQUIPMENT_COPPER_REINFORCED_PICK,
            "copper-reinforced stone pick",
            Mass::from_milligrams(1_020_000),
            profile([
                (
                    CAPABILITY_MINING_FLOW,
                    CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(30_000)),
                ),
                (
                    CAPABILITY_MINING_MAX_BATCH,
                    CapabilityValue::Mass(Mass::from_milligrams(300_000)),
                ),
                (
                    CAPABILITY_MINING_MAX_HARDNESS,
                    CapabilityValue::Pressure(Pressure::from_pascals(750_000_000)),
                ),
            ]),
            thresholds(),
            vec![reinforced_mining_curve],
        )
        .with_assembly_profile(MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::new(
                CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
                Mass::from_milligrams(800_000),
            ),
            MaterialInputSpec::new(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(200_000),
            ),
            MaterialInputSpec::new(
                CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
                Mass::from_milligrams(20_000),
            ),
        ]))
        .with_worn_recovery_form(FORM_SCRAP)
        .with_upgrade_profile(EquipmentUpgradeProfile::new(
            EQUIPMENT_STONE_PICK,
            MaterialAssemblyProfile::new(vec![MaterialInputSpec::new(
                CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
                Mass::from_milligrams(20_000),
            )]),
        )),
        EquipmentDefinition::new_with_capability_condition_curves(
            EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
            "copper-reinforced stone hand crank",
            Mass::from_milligrams(1_120_000),
            profile([(
                CAPABILITY_MANUAL_POWER_OUTPUT,
                CapabilityValue::Power(Power::from_microwatts(100_000_000)),
            )]),
            thresholds(),
            vec![reinforced_hand_crank_curve],
        )
        .with_assembly_profile(MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::new(
                CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
                Mass::from_milligrams(900_000),
            ),
            MaterialInputSpec::new(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(200_000),
            ),
            MaterialInputSpec::new(
                CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
                Mass::from_milligrams(20_000),
            ),
        ]))
        .with_worn_recovery_form(FORM_SCRAP)
        .with_upgrade_profile(EquipmentUpgradeProfile::new(
            EQUIPMENT_STONE_HAND_CRANK,
            MaterialAssemblyProfile::new(vec![MaterialInputSpec::new(
                CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
                Mass::from_milligrams(20_000),
            )]),
        )),
        EquipmentDefinition::new_with_capability_condition_curves(
            EQUIPMENT_STONE_CRUSHER,
            "stone toggle crusher",
            Mass::from_milligrams(3_000_000),
            profile([
                (
                    CAPABILITY_CRUSHER_FLOW,
                    CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(5_000)),
                ),
                (
                    CAPABILITY_CRUSHER_BATCH,
                    CapabilityValue::Mass(Mass::from_milligrams(1_000_000)),
                ),
            ]),
            thresholds(),
            vec![stone_crusher_curve],
        )
        .with_assembly_profile(MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::new(
                CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
                Mass::from_milligrams(2_400_000),
            ),
            MaterialInputSpec::new(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(600_000),
            ),
        ]))
        .with_worn_recovery_form(FORM_SCRAP),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::equipment::resolve_equipment_capability;

    #[test]
    fn failed_thermal_equipment_has_no_productive_heat_transfer_rate() {
        let registry = build_equipment_registry();
        for (equipment, capability) in [
            (EQUIPMENT_ELECTRIC_FURNACE, CAPABILITY_HEATING_POWER),
            (EQUIPMENT_CASTING_MOLD, CAPABILITY_COOLING_POWER),
        ] {
            let definition = registry
                .get_equipment(equipment)
                .unwrap_or_else(|| panic!("built-in thermal equipment disappeared"));
            assert_eq!(
                resolve_equipment_capability(definition, Condition::FAILED, capability),
                Some(CapabilityValue::Power(Power::ZERO))
            );
        }
    }

    #[test]
    fn every_builtin_equipment_definition_has_a_condition_recovery_route() {
        let registry = build_equipment_registry();
        for definition in registry.definitions() {
            assert!(
                definition.maintenance_profile().is_some()
                    || definition.worn_recovery_form().is_some(),
                "built-in equipment {} must be repairable or destructively recoverable after wear",
                definition.id().value()
            );
        }
    }

    #[test]
    fn industrial_machines_are_fixed_while_primitive_equipment_remains_portable() {
        let registry = build_equipment_registry();
        for equipment in [
            EQUIPMENT_JAW_CRUSHER,
            EQUIPMENT_ELECTRIC_FURNACE,
            EQUIPMENT_CASTING_MOLD,
            EQUIPMENT_DRY_SCREEN,
            EQUIPMENT_GRINDING_MILL,
        ] {
            assert!(
                registry
                    .get_equipment(equipment)
                    .is_some_and(|definition| definition.requires_structural_support()),
                "industrial equipment {} must require structural installation",
                equipment.value()
            );
        }
        for equipment in [
            EQUIPMENT_STONE_PICK,
            EQUIPMENT_STONE_HAND_CRANK,
            EQUIPMENT_COPPER_REINFORCED_PICK,
            EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
            EQUIPMENT_STONE_CRUSHER,
        ] {
            assert!(
                registry
                    .get_equipment(equipment)
                    .is_some_and(|definition| !definition.requires_structural_support()),
                "primitive equipment {} must remain portable",
                equipment.value()
            );
        }
    }
}
