//! Primitive ore-processing equipment and copper-reinforced variants.

use crate::capability::CapabilityValue;
use crate::core::quantity::{Mass, MassFlow};
use crate::equipment::EquipmentDefinition;
use crate::material::{CommodityKey, MaterialAssemblyProfile, MaterialInputSpec};

use crate::content::capabilities::{
    CAPABILITY_CRUSHER_BATCH, CAPABILITY_CRUSHER_FLOW, CAPABILITY_SEPARATOR_BATCH,
    CAPABILITY_SEPARATOR_FLOW,
};
use crate::content::materials::{
    FORM_HANDLE, FORM_SCRAP, FORM_TOOL, MATERIAL_STONE, MATERIAL_WOOD,
};

use super::super::authoring::{
    component_maintenance, mass_condition_curve, mass_flow_condition_curve, profile, thresholds,
};
use super::super::{
    EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER, EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR,
    EQUIPMENT_STONE_CRUSHER, EQUIPMENT_STONE_SEPARATOR,
};
use super::{copper_reinforcement_input, copper_upgrade};

pub(super) fn stone_crusher() -> EquipmentDefinition {
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

pub(super) fn stone_separator() -> EquipmentDefinition {
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

pub(super) fn copper_reinforced_stone_crusher() -> EquipmentDefinition {
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

pub(super) fn copper_reinforced_stone_separator() -> EquipmentDefinition {
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
