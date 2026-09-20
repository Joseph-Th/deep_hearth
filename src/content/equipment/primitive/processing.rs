//! Primitive ore-processing equipment and copper-reinforced variants.

use crate::capability::CapabilityValue;
use crate::core::quantity::{Mass, MassFlow};
use crate::equipment::{EquipmentDefinition, EquipmentUpgradeProfile};
use crate::material::{CommodityKey, MaterialAssemblyProfile, MaterialInputSpec};

use crate::content::capabilities::{
    CAPABILITY_CRUSHER_BATCH, CAPABILITY_CRUSHER_FLOW, CAPABILITY_GRINDER_BATCH,
    CAPABILITY_GRINDER_FLOW, CAPABILITY_SCREEN_BATCH, CAPABILITY_SCREEN_FLOW,
    CAPABILITY_SEPARATOR_BATCH, CAPABILITY_SEPARATOR_FLOW,
};
use crate::content::crafted_parts::{COPPER_SCREEN_PLATE_MASS, TIMBER_RIDDLE_PANEL_MASS};
use crate::content::materials::{
    FORM_HANDLE, FORM_SCREEN_PLATE, FORM_TIMBER_RIDDLE_PANEL, FORM_TOOL, MATERIAL_COPPER,
    MATERIAL_STONE, MATERIAL_WOOD,
};

use super::super::authoring::{
    EquipmentDefinitionAuthoringExt, assembled_definition_with_condition_curves,
    mass_condition_curve, mass_flow_condition_curve, profile, thresholds,
};
use super::super::{
    EQUIPMENT_COPPER_PLATE_SIZING_SCREEN, EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER,
    EQUIPMENT_COPPER_REINFORCED_STONE_ROTARY_QUERN, EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR,
    EQUIPMENT_STONE_CRUSHER, EQUIPMENT_STONE_ROTARY_QUERN, EQUIPMENT_STONE_SEPARATOR,
    EQUIPMENT_TIMBER_RIDDLE_SIZING_SCREEN,
};
use super::{copper_reinforcement_input, copper_upgrade};

mod settlement;
pub(super) use settlement::{timber_frame_comminution_mill, timber_ore_dressing_table};

pub(super) fn stone_crusher() -> EquipmentDefinition {
    assembled_definition_with_condition_curves(
        EQUIPMENT_STONE_CRUSHER,
        "stone toggle crusher",
        MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
                Mass::from_milligrams(1_600_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(400_000),
            ),
        ]),
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
    .with_assembly_component_maintenance(CommodityKey::new(MATERIAL_STONE, FORM_TOOL))
}

pub(super) fn stone_separator() -> EquipmentDefinition {
    assembled_definition_with_condition_curves(
        EQUIPMENT_STONE_SEPARATOR,
        "stone rocking separator",
        MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
                Mass::from_milligrams(800_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(400_000),
            ),
        ]),
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
    .with_assembly_component_maintenance(CommodityKey::new(MATERIAL_STONE, FORM_TOOL))
}

pub(super) fn stone_rotary_quern() -> EquipmentDefinition {
    assembled_definition_with_condition_curves(
        EQUIPMENT_STONE_ROTARY_QUERN,
        "stone rotary quern",
        MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
                Mass::from_milligrams(1_600_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(200_000),
            ),
        ]),
        profile([
            (
                CAPABILITY_GRINDER_FLOW,
                CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(1_200)),
            ),
            (
                CAPABILITY_GRINDER_BATCH,
                CapabilityValue::Mass(Mass::from_milligrams(500_000)),
            ),
        ]),
        thresholds(),
        vec![
            mass_flow_condition_curve(
                CAPABILITY_GRINDER_FLOW,
                600_000,
                MassFlow::from_milligrams_per_second(600),
            ),
            mass_condition_curve(
                CAPABILITY_GRINDER_BATCH,
                600_000,
                Mass::from_milligrams(250_000),
            ),
        ],
    )
    .with_assembly_component_maintenance(CommodityKey::new(MATERIAL_STONE, FORM_TOOL))
}

/// A low-tech mechanically shaken riddle with a replaceable slatted timber sizing panel. It opens
/// the same physical grind/screen/regrind loop as later screens without consuming scarce copper,
/// but pays for that accessibility with smaller batches, lower throughput, and a heavy wear part.
pub(super) fn timber_riddle_sizing_screen() -> EquipmentDefinition {
    assembled_definition_with_condition_curves(
        EQUIPMENT_TIMBER_RIDDLE_SIZING_SCREEN,
        "timber riddle sizing screen",
        MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_TIMBER_RIDDLE_PANEL),
                TIMBER_RIDDLE_PANEL_MASS,
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(200_000),
            ),
        ]),
        profile([
            (
                CAPABILITY_SCREEN_FLOW,
                CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(1_250)),
            ),
            (
                CAPABILITY_SCREEN_BATCH,
                CapabilityValue::Mass(Mass::from_milligrams(250_000)),
            ),
        ]),
        thresholds(),
        vec![
            mass_flow_condition_curve(
                CAPABILITY_SCREEN_FLOW,
                600_000,
                MassFlow::from_milligrams_per_second(625),
            ),
            mass_condition_curve(
                CAPABILITY_SCREEN_BATCH,
                600_000,
                Mass::from_milligrams(125_000),
            ),
        ],
    )
    .with_assembly_component_maintenance(CommodityKey::new(MATERIAL_WOOD, FORM_TIMBER_RIDDLE_PANEL))
}

pub(super) fn copper_plate_sizing_screen() -> EquipmentDefinition {
    assembled_definition_with_condition_curves(
        EQUIPMENT_COPPER_PLATE_SIZING_SCREEN,
        "timber-framed copper shaker screen",
        MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_TIMBER_RIDDLE_PANEL),
                TIMBER_RIDDLE_PANEL_MASS,
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(200_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_COPPER, FORM_SCREEN_PLATE),
                COPPER_SCREEN_PLATE_MASS,
            ),
        ]),
        profile([
            (
                CAPABILITY_SCREEN_FLOW,
                CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(2_500)),
            ),
            (
                CAPABILITY_SCREEN_BATCH,
                CapabilityValue::Mass(Mass::from_milligrams(500_000)),
            ),
        ]),
        thresholds(),
        vec![
            mass_flow_condition_curve(
                CAPABILITY_SCREEN_FLOW,
                600_000,
                MassFlow::from_milligrams_per_second(1_250),
            ),
            mass_condition_curve(
                CAPABILITY_SCREEN_BATCH,
                600_000,
                Mass::from_milligrams(250_000),
            ),
        ],
    )
    .with_assembly_component_maintenance(CommodityKey::new(MATERIAL_WOOD, FORM_TIMBER_RIDDLE_PANEL))
    .with_upgrade_profile(EquipmentUpgradeProfile::new(
        EQUIPMENT_TIMBER_RIDDLE_SIZING_SCREEN,
        MaterialAssemblyProfile::new(vec![MaterialInputSpec::pure(
            CommodityKey::new(MATERIAL_COPPER, FORM_SCREEN_PLATE),
            COPPER_SCREEN_PLATE_MASS,
        )]),
    ))
}

pub(super) fn copper_reinforced_stone_crusher() -> EquipmentDefinition {
    assembled_definition_with_condition_curves(
        EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER,
        "copper-reinforced stone toggle crusher",
        MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
                Mass::from_milligrams(1_600_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(400_000),
            ),
            copper_reinforcement_input(),
        ]),
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
    .with_assembly_component_maintenance(CommodityKey::new(MATERIAL_STONE, FORM_TOOL))
    .with_upgrade_profile(copper_upgrade(EQUIPMENT_STONE_CRUSHER))
}

pub(super) fn copper_reinforced_stone_rotary_quern() -> EquipmentDefinition {
    assembled_definition_with_condition_curves(
        EQUIPMENT_COPPER_REINFORCED_STONE_ROTARY_QUERN,
        "copper-reinforced stone rotary quern",
        MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
                Mass::from_milligrams(1_600_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(200_000),
            ),
            copper_reinforcement_input(),
        ]),
        profile([
            (
                CAPABILITY_GRINDER_FLOW,
                CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(1_800)),
            ),
            (
                CAPABILITY_GRINDER_BATCH,
                CapabilityValue::Mass(Mass::from_milligrams(750_000)),
            ),
        ]),
        thresholds(),
        vec![
            mass_flow_condition_curve(
                CAPABILITY_GRINDER_FLOW,
                600_000,
                MassFlow::from_milligrams_per_second(900),
            ),
            mass_condition_curve(
                CAPABILITY_GRINDER_BATCH,
                600_000,
                Mass::from_milligrams(375_000),
            ),
        ],
    )
    .with_assembly_component_maintenance(CommodityKey::new(MATERIAL_STONE, FORM_TOOL))
    .with_upgrade_profile(copper_upgrade(EQUIPMENT_STONE_ROTARY_QUERN))
}

pub(super) fn copper_reinforced_stone_separator() -> EquipmentDefinition {
    assembled_definition_with_condition_curves(
        EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR,
        "copper-reinforced stone rocking separator",
        MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
                Mass::from_milligrams(800_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(400_000),
            ),
            copper_reinforcement_input(),
        ]),
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
    .with_assembly_component_maintenance(CommodityKey::new(MATERIAL_STONE, FORM_TOOL))
    .with_upgrade_profile(copper_upgrade(EQUIPMENT_STONE_SEPARATOR))
}
