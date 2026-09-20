//! Settlement-scale ore-processing equipment that consolidates portable specialist roles.

use crate::capability::CapabilityValue;
use crate::content::capabilities::{
    CAPABILITY_CRUSHER_BATCH, CAPABILITY_CRUSHER_FLOW, CAPABILITY_GRINDER_BATCH,
    CAPABILITY_GRINDER_FLOW, CAPABILITY_SCREEN_BATCH, CAPABILITY_SCREEN_FLOW,
    CAPABILITY_SEPARATOR_BATCH, CAPABILITY_SEPARATOR_FLOW,
};
use crate::content::crafted_parts::{COPPER_SCREEN_PLATE_MASS, TIMBER_RIDDLE_PANEL_MASS};
use crate::content::materials::{
    FORM_BOARD, FORM_HANDLE, FORM_SCREEN_PLATE, FORM_TIMBER_RIDDLE_PANEL, FORM_TOOL,
    MATERIAL_COPPER, MATERIAL_STONE, MATERIAL_WOOD,
};
use crate::core::quantity::{Mass, MassFlow};
use crate::equipment::EquipmentDefinition;
use crate::material::{CommodityKey, MaterialAssemblyProfile, MaterialInputSpec};

use super::super::super::authoring::{
    EquipmentDefinitionAuthoringExt, assembled_definition_with_condition_curves,
    mass_condition_curve, mass_flow_condition_curve, profile, thresholds,
};
use super::super::super::{
    EQUIPMENT_TIMBER_FRAME_COMMINUTION_MILL, EQUIPMENT_TIMBER_ORE_DRESSING_TABLE,
};
use super::super::copper_reinforcement_input;

/// A settlement-scale machine that puts a heavy stone breaker and rotary burr in one timber frame.
/// It deliberately consolidates two process roles instead of replacing their portable providers:
/// the mill buys larger batches and higher throughput with much more bulk material, while one
/// equipment instance cannot crush and grind at the same time. A small copper bearing set keeps the
/// scarce-metal cost below building both portable copper upgrades, leaving a genuine choice between
/// compact specialization, parallel capacity, and a larger shared workshop investment.
pub(in crate::content::equipment::primitive) fn timber_frame_comminution_mill()
-> EquipmentDefinition {
    assembled_definition_with_condition_curves(
        EQUIPMENT_TIMBER_FRAME_COMMINUTION_MILL,
        "timber-framed stone comminution mill",
        MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
                Mass::from_milligrams(3_200_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
                Mass::from_milligrams(3_200_000),
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
                CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(4_500)),
            ),
            (
                CAPABILITY_CRUSHER_BATCH,
                CapabilityValue::Mass(Mass::from_milligrams(2_000_000)),
            ),
            (
                CAPABILITY_GRINDER_FLOW,
                CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(2_400)),
            ),
            (
                CAPABILITY_GRINDER_BATCH,
                CapabilityValue::Mass(Mass::from_milligrams(1_000_000)),
            ),
        ]),
        thresholds(),
        vec![
            mass_flow_condition_curve(
                CAPABILITY_CRUSHER_FLOW,
                600_000,
                MassFlow::from_milligrams_per_second(2_250),
            ),
            mass_condition_curve(
                CAPABILITY_CRUSHER_BATCH,
                600_000,
                Mass::from_milligrams(1_000_000),
            ),
            mass_flow_condition_curve(
                CAPABILITY_GRINDER_FLOW,
                600_000,
                MassFlow::from_milligrams_per_second(1_200),
            ),
            mass_condition_curve(
                CAPABILITY_GRINDER_BATCH,
                600_000,
                Mass::from_milligrams(500_000),
            ),
        ],
    )
    .with_assembly_component_maintenance(CommodityKey::new(MATERIAL_STONE, FORM_TOOL))
}

/// A dry ore-dressing bench combining a replaceable sizing deck with a rocking stone separation
/// bed. It consolidates screening and gravity separation into one copper-era settlement asset, but
/// its shared occupancy prevents both stages from running in parallel. The larger timber frame and
/// dual copper fittings therefore compete with keeping the lighter screen and separator as two
/// independent machines rather than making them obsolete.
pub(in crate::content::equipment::primitive) fn timber_ore_dressing_table() -> EquipmentDefinition {
    assembled_definition_with_condition_curves(
        EQUIPMENT_TIMBER_ORE_DRESSING_TABLE,
        "timber ore-dressing table",
        MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
                Mass::from_milligrams(2_400_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
                Mass::from_milligrams(800_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(400_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_TIMBER_RIDDLE_PANEL),
                TIMBER_RIDDLE_PANEL_MASS,
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_COPPER, FORM_SCREEN_PLATE),
                COPPER_SCREEN_PLATE_MASS,
            ),
            copper_reinforcement_input(),
        ]),
        profile([
            (
                CAPABILITY_SCREEN_FLOW,
                CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(3_500)),
            ),
            (
                CAPABILITY_SCREEN_BATCH,
                CapabilityValue::Mass(Mass::from_milligrams(1_000_000)),
            ),
            (
                CAPABILITY_SEPARATOR_FLOW,
                CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(6_000)),
            ),
            (
                CAPABILITY_SEPARATOR_BATCH,
                CapabilityValue::Mass(Mass::from_milligrams(1_250_000)),
            ),
        ]),
        thresholds(),
        vec![
            mass_flow_condition_curve(
                CAPABILITY_SCREEN_FLOW,
                600_000,
                MassFlow::from_milligrams_per_second(1_750),
            ),
            mass_condition_curve(
                CAPABILITY_SCREEN_BATCH,
                600_000,
                Mass::from_milligrams(500_000),
            ),
            mass_flow_condition_curve(
                CAPABILITY_SEPARATOR_FLOW,
                600_000,
                MassFlow::from_milligrams_per_second(3_000),
            ),
            mass_condition_curve(
                CAPABILITY_SEPARATOR_BATCH,
                600_000,
                Mass::from_milligrams(625_000),
            ),
        ],
    )
    .with_assembly_component_maintenance(CommodityKey::new(MATERIAL_WOOD, FORM_TIMBER_RIDDLE_PANEL))
}
