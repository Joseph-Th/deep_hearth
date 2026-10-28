//! Portable, material-backed cutting tools between bare knapping and installed workshops.

use crate::capability::CapabilityValue;
use crate::core::quantity::{Mass, MassFlow};
use crate::equipment::EquipmentDefinition;
use crate::material::{CommodityKey, MaterialAssemblyProfile, MaterialInputSpec};

use crate::content::capabilities::{
    CAPABILITY_SAWING_FLOW, CAPABILITY_STONE_CHISELING_FLOW, CAPABILITY_WOODWORKING_FLOW,
};
use crate::content::crafted_parts::COPPER_SAW_BLADE_MASS;
use crate::content::materials::{
    FORM_HANDLE, FORM_SAW_BLADE, FORM_TOOL, MATERIAL_COPPER, MATERIAL_FLINT, MATERIAL_WOOD,
};

use super::super::authoring::{
    EquipmentDefinitionAuthoringExt, assembled_definition_with_condition_curves,
    mass_flow_condition_curve, profile, thresholds,
};
use super::super::{
    EQUIPMENT_COPPER_HAND_SAW, EQUIPMENT_COPPER_STONE_CHISEL, EQUIPMENT_FLINT_WOODWORKING_KNIFE,
};
use super::copper_reinforcement_input;

/// The small, sharp flint edge can be carried on a first expedition. Its limited
/// material supply and replaceable head compete with the heavier common-stone adze.
pub(super) fn flint_woodworking_knife() -> EquipmentDefinition {
    assembled_definition_with_condition_curves(
        EQUIPMENT_FLINT_WOODWORKING_KNIFE,
        "hafted flint woodworking knife",
        MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_FLINT, FORM_TOOL),
                Mass::from_milligrams(220_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(200_000),
            ),
        ]),
        profile([(
            CAPABILITY_WOODWORKING_FLOW,
            CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(14_000)),
        )]),
        thresholds(),
        vec![mass_flow_condition_curve(
            CAPABILITY_WOODWORKING_FLOW,
            500_000,
            MassFlow::from_milligrams_per_second(7_000),
        )],
    )
    .with_assembly_component_maintenance(CommodityKey::new(MATERIAL_FLINT, FORM_TOOL))
}

/// A portable saw uses the same physically made copper blade as the later bench,
/// but its unsupported hand stroke has substantially lower throughput. It buys
/// 90% board recovery before a permanent settlement machine is worthwhile.
pub(super) fn copper_hand_saw() -> EquipmentDefinition {
    assembled_definition_with_condition_curves(
        EQUIPMENT_COPPER_HAND_SAW,
        "handheld copper frame saw",
        MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_COPPER, FORM_SAW_BLADE),
                COPPER_SAW_BLADE_MASS,
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(200_000),
            ),
        ]),
        profile([(
            CAPABILITY_SAWING_FLOW,
            CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(15_000)),
        )]),
        thresholds(),
        vec![mass_flow_condition_curve(
            CAPABILITY_SAWING_FLOW,
            500_000,
            MassFlow::from_milligrams_per_second(7_500),
        )],
    )
    .with_assembly_component_maintenance(CommodityKey::new(MATERIAL_COPPER, FORM_SAW_BLADE))
}

/// A reusable hand stone chisel consumes scarce cold-worked copper reinforcement.
/// The player chooses to invest that copper only when repeated precise crock
/// carving can repay the tool; the generic 180-tick route remains available.
pub(super) fn copper_stone_chisel() -> EquipmentDefinition {
    assembled_definition_with_condition_curves(
        EQUIPMENT_COPPER_STONE_CHISEL,
        "hafted copper stone chisel",
        MaterialAssemblyProfile::new(vec![
            copper_reinforcement_input(),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(200_000),
            ),
        ]),
        profile([(
            CAPABILITY_STONE_CHISELING_FLOW,
            CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(8_000)),
        )]),
        thresholds(),
        vec![mass_flow_condition_curve(
            CAPABILITY_STONE_CHISELING_FLOW,
            500_000,
            MassFlow::from_milligrams_per_second(5_000),
        )],
    )
    .with_assembly_component_maintenance(CommodityKey::new(
        MATERIAL_COPPER,
        crate::content::FORM_REINFORCEMENT,
    ))
}
