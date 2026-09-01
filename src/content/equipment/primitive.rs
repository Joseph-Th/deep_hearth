//! Portable primitive equipment and additive copper upgrade definitions.

use crate::capability::CapabilityValue;
use crate::core::quantity::{Mass, MassFlow, Power, Pressure};
use crate::equipment::{EquipmentDefinition, EquipmentDefinitionId, EquipmentUpgradeProfile};
use crate::material::{CommodityKey, MaterialAssemblyProfile, MaterialInputSpec};

use super::authoring::{
    component_maintenance, mass_condition_curve, mass_flow_condition_curve, power_condition_curve,
    profile, thresholds,
};
use super::{
    EQUIPMENT_COPPER_REINFORCED_HAND_CRANK, EQUIPMENT_COPPER_REINFORCED_PICK,
    EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER, EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR,
    EQUIPMENT_STONE_CRUSHER, EQUIPMENT_STONE_HAND_CRANK, EQUIPMENT_STONE_PICK,
    EQUIPMENT_STONE_SEPARATOR,
};
use crate::content::capabilities::{
    CAPABILITY_CRUSHER_BATCH, CAPABILITY_CRUSHER_FLOW, CAPABILITY_MANUAL_POWER_OUTPUT,
    CAPABILITY_MINING_FLOW, CAPABILITY_MINING_MAX_BATCH, CAPABILITY_MINING_MAX_HARDNESS,
    CAPABILITY_SEPARATOR_BATCH, CAPABILITY_SEPARATOR_FLOW,
};
use crate::content::materials::{
    FORM_FLYWHEEL, FORM_HANDLE, FORM_REINFORCEMENT, FORM_SCRAP, FORM_TOOL, MATERIAL_COPPER,
    MATERIAL_STONE, MATERIAL_WOOD,
};

const COPPER_REINFORCEMENT_MASS: Mass = Mass::from_milligrams(20_000);

pub(super) fn definitions() -> [EquipmentDefinition; 8] {
    [
        stone_pick(),
        stone_hand_crank(),
        copper_reinforced_pick(),
        copper_reinforced_hand_crank(),
        stone_crusher(),
        stone_separator(),
        copper_reinforced_stone_crusher(),
        copper_reinforced_stone_separator(),
    ]
}

fn copper_reinforcement_input() -> MaterialInputSpec {
    MaterialInputSpec::pure(
        CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
        COPPER_REINFORCEMENT_MASS,
    )
}

fn copper_upgrade(from: EquipmentDefinitionId) -> EquipmentUpgradeProfile {
    EquipmentUpgradeProfile::new(
        from,
        MaterialAssemblyProfile::new(vec![copper_reinforcement_input()]),
    )
}

fn stone_pick() -> EquipmentDefinition {
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
        vec![mass_flow_condition_curve(
            CAPABILITY_MINING_FLOW,
            500_000,
            MassFlow::from_milligrams_per_second(10_000),
        )],
    )
    .with_assembly_profile(MaterialAssemblyProfile::new(vec![
        MaterialInputSpec::pure(
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            Mass::from_milligrams(800_000),
        ),
        MaterialInputSpec::pure(
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            Mass::from_milligrams(200_000),
        ),
    ]))
    .with_maintenance_profile(component_maintenance(
        CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
        Mass::from_milligrams(800_000),
    ))
    .with_worn_recovery_form(FORM_SCRAP)
}

fn stone_hand_crank() -> EquipmentDefinition {
    EquipmentDefinition::new_with_capability_condition_curves(
        EQUIPMENT_STONE_HAND_CRANK,
        "stone hand crank",
        Mass::from_milligrams(1_100_000),
        profile([(
            CAPABILITY_MANUAL_POWER_OUTPUT,
            CapabilityValue::Power(Power::from_microwatts(50_000_000)),
        )]),
        thresholds(),
        vec![power_condition_curve(
            CAPABILITY_MANUAL_POWER_OUTPUT,
            500_000,
            Power::from_microwatts(25_000_000),
        )],
    )
    .with_assembly_profile(MaterialAssemblyProfile::new(vec![
        MaterialInputSpec::pure(
            CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
            Mass::from_milligrams(900_000),
        ),
        MaterialInputSpec::pure(
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            Mass::from_milligrams(200_000),
        ),
    ]))
    .with_maintenance_profile(component_maintenance(
        CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
        Mass::from_milligrams(200_000),
    ))
    .with_worn_recovery_form(FORM_SCRAP)
}

fn copper_reinforced_pick() -> EquipmentDefinition {
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
        vec![mass_flow_condition_curve(
            CAPABILITY_MINING_FLOW,
            500_000,
            MassFlow::from_milligrams_per_second(15_000),
        )],
    )
    .with_assembly_profile(MaterialAssemblyProfile::new(vec![
        MaterialInputSpec::pure(
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            Mass::from_milligrams(800_000),
        ),
        MaterialInputSpec::pure(
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            Mass::from_milligrams(200_000),
        ),
        copper_reinforcement_input(),
    ]))
    .with_maintenance_profile(component_maintenance(
        CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
        Mass::from_milligrams(800_000),
    ))
    .with_worn_recovery_form(FORM_SCRAP)
    .with_upgrade_profile(copper_upgrade(EQUIPMENT_STONE_PICK))
}

fn copper_reinforced_hand_crank() -> EquipmentDefinition {
    EquipmentDefinition::new_with_capability_condition_curves(
        EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
        "copper-reinforced stone hand crank",
        Mass::from_milligrams(1_120_000),
        profile([(
            CAPABILITY_MANUAL_POWER_OUTPUT,
            CapabilityValue::Power(Power::from_microwatts(150_000_000)),
        )]),
        thresholds(),
        vec![power_condition_curve(
            CAPABILITY_MANUAL_POWER_OUTPUT,
            500_000,
            Power::from_microwatts(75_000_000),
        )],
    )
    .with_assembly_profile(MaterialAssemblyProfile::new(vec![
        MaterialInputSpec::pure(
            CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
            Mass::from_milligrams(900_000),
        ),
        MaterialInputSpec::pure(
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            Mass::from_milligrams(200_000),
        ),
        copper_reinforcement_input(),
    ]))
    .with_maintenance_profile(component_maintenance(
        CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
        Mass::from_milligrams(200_000),
    ))
    .with_worn_recovery_form(FORM_SCRAP)
    .with_upgrade_profile(copper_upgrade(EQUIPMENT_STONE_HAND_CRANK))
}

fn stone_crusher() -> EquipmentDefinition {
    EquipmentDefinition::new_with_capability_condition_curves(
        EQUIPMENT_STONE_CRUSHER,
        "stone toggle crusher",
        Mass::from_milligrams(2_000_000),
        profile([
            (
                CAPABILITY_CRUSHER_FLOW,
                CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(2_000)),
            ),
            (
                CAPABILITY_CRUSHER_BATCH,
                CapabilityValue::Mass(Mass::from_milligrams(1_000_000)),
            ),
        ]),
        thresholds(),
        vec![
            mass_flow_condition_curve(
                CAPABILITY_CRUSHER_FLOW,
                600_000,
                MassFlow::from_milligrams_per_second(1_000),
            ),
            mass_condition_curve(
                CAPABILITY_CRUSHER_BATCH,
                600_000,
                Mass::from_milligrams(500_000),
            ),
        ],
    )
    .with_assembly_profile(MaterialAssemblyProfile::new(vec![
        MaterialInputSpec::pure(
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            Mass::from_milligrams(1_600_000),
        ),
        MaterialInputSpec::pure(
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            Mass::from_milligrams(400_000),
        ),
    ]))
    .with_maintenance_profile(component_maintenance(
        CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
        Mass::from_milligrams(1_600_000),
    ))
    .with_worn_recovery_form(FORM_SCRAP)
}

fn stone_separator() -> EquipmentDefinition {
    EquipmentDefinition::new_with_capability_condition_curves(
        EQUIPMENT_STONE_SEPARATOR,
        "stone rocking separator",
        Mass::from_milligrams(1_200_000),
        profile([
            (
                CAPABILITY_SEPARATOR_FLOW,
                CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(3_000)),
            ),
            (
                CAPABILITY_SEPARATOR_BATCH,
                CapabilityValue::Mass(Mass::from_milligrams(500_000)),
            ),
        ]),
        thresholds(),
        vec![
            mass_flow_condition_curve(
                CAPABILITY_SEPARATOR_FLOW,
                600_000,
                MassFlow::from_milligrams_per_second(1_500),
            ),
            mass_condition_curve(
                CAPABILITY_SEPARATOR_BATCH,
                600_000,
                Mass::from_milligrams(250_000),
            ),
        ],
    )
    .with_assembly_profile(MaterialAssemblyProfile::new(vec![
        MaterialInputSpec::pure(
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            Mass::from_milligrams(800_000),
        ),
        MaterialInputSpec::pure(
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            Mass::from_milligrams(400_000),
        ),
    ]))
    .with_maintenance_profile(component_maintenance(
        CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
        Mass::from_milligrams(800_000),
    ))
    .with_worn_recovery_form(FORM_SCRAP)
}

fn copper_reinforced_stone_crusher() -> EquipmentDefinition {
    EquipmentDefinition::new_with_capability_condition_curves(
        EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER,
        "copper-reinforced stone toggle crusher",
        Mass::from_milligrams(2_020_000),
        profile([
            (
                CAPABILITY_CRUSHER_FLOW,
                CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(3_000)),
            ),
            (
                CAPABILITY_CRUSHER_BATCH,
                CapabilityValue::Mass(Mass::from_milligrams(1_500_000)),
            ),
        ]),
        thresholds(),
        vec![
            mass_flow_condition_curve(
                CAPABILITY_CRUSHER_FLOW,
                600_000,
                MassFlow::from_milligrams_per_second(1_500),
            ),
            mass_condition_curve(
                CAPABILITY_CRUSHER_BATCH,
                600_000,
                Mass::from_milligrams(750_000),
            ),
        ],
    )
    .with_assembly_profile(MaterialAssemblyProfile::new(vec![
        MaterialInputSpec::pure(
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            Mass::from_milligrams(1_600_000),
        ),
        MaterialInputSpec::pure(
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            Mass::from_milligrams(400_000),
        ),
        copper_reinforcement_input(),
    ]))
    .with_maintenance_profile(component_maintenance(
        CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
        Mass::from_milligrams(1_600_000),
    ))
    .with_worn_recovery_form(FORM_SCRAP)
    .with_upgrade_profile(copper_upgrade(EQUIPMENT_STONE_CRUSHER))
}

fn copper_reinforced_stone_separator() -> EquipmentDefinition {
    EquipmentDefinition::new_with_capability_condition_curves(
        EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR,
        "copper-reinforced stone rocking separator",
        Mass::from_milligrams(1_220_000),
        profile([
            (
                CAPABILITY_SEPARATOR_FLOW,
                CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(4_500)),
            ),
            (
                CAPABILITY_SEPARATOR_BATCH,
                CapabilityValue::Mass(Mass::from_milligrams(750_000)),
            ),
        ]),
        thresholds(),
        vec![
            mass_flow_condition_curve(
                CAPABILITY_SEPARATOR_FLOW,
                600_000,
                MassFlow::from_milligrams_per_second(2_250),
            ),
            mass_condition_curve(
                CAPABILITY_SEPARATOR_BATCH,
                600_000,
                Mass::from_milligrams(375_000),
            ),
        ],
    )
    .with_assembly_profile(MaterialAssemblyProfile::new(vec![
        MaterialInputSpec::pure(
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            Mass::from_milligrams(800_000),
        ),
        MaterialInputSpec::pure(
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            Mass::from_milligrams(400_000),
        ),
        copper_reinforcement_input(),
    ]))
    .with_maintenance_profile(component_maintenance(
        CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
        Mass::from_milligrams(800_000),
    ))
    .with_worn_recovery_form(FORM_SCRAP)
    .with_upgrade_profile(copper_upgrade(EQUIPMENT_STONE_SEPARATOR))
}
