//! Direct-labor ore-dressing tools between bare-hand work and powered machinery.

use crate::capability::CapabilityValue;
use crate::core::quantity::{Mass, MassFlow};
use crate::equipment::EquipmentDefinition;
use crate::material::{CommodityKey, MaterialAssemblyProfile, MaterialInputSpec};

use crate::content::capabilities::{CAPABILITY_COBBING_FLOW, CAPABILITY_ORE_PICKING_FLOW};
use crate::content::materials::{
    FORM_BOARD, FORM_HANDLE, FORM_TOOL, MATERIAL_STONE, MATERIAL_WOOD,
};

use super::super::authoring::{
    EquipmentDefinitionAuthoringExt, assembled_definition_with_condition_curves,
    mass_flow_condition_curve, profile, thresholds,
};
use super::super::{EQUIPMENT_STONE_COBBING_HAMMER, EQUIPMENT_TIMBER_DRESSING_BENCH};

/// Portable cobbing hammer for breaking selected ore lumps on a hard surface without building a
/// powered crusher. It improves attention cost only; particle-size and recovery physics remain
/// owned by the manual comminution process.
pub(super) fn stone_cobbing_hammer() -> EquipmentDefinition {
    assembled_definition_with_condition_curves(
        EQUIPMENT_STONE_COBBING_HAMMER,
        "hafted stone cobbing hammer",
        MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
                Mass::from_milligrams(600_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(200_000),
            ),
        ]),
        profile([(
            CAPABILITY_COBBING_FLOW,
            CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(750)),
        )]),
        thresholds(),
        vec![mass_flow_condition_curve(
            CAPABILITY_COBBING_FLOW,
            500_000,
            MassFlow::from_milligrams_per_second(375),
        )],
    )
    .with_assembly_component_maintenance(CommodityKey::new(MATERIAL_STONE, FORM_TOOL))
}

/// Settlement dressing bench combining a replaceable stone working face, cobbing hammer, and broad
/// timber picking surface. It earns its larger material commitment by accelerating both manual
/// breaking and visible native-copper sorting while still occupying the player's attention.
pub(super) fn timber_dressing_bench() -> EquipmentDefinition {
    assembled_definition_with_condition_curves(
        EQUIPMENT_TIMBER_DRESSING_BENCH,
        "timber cobbing and picking bench",
        MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
                Mass::from_milligrams(1_600_000),
            ),
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
                CAPABILITY_COBBING_FLOW,
                CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(1_250)),
            ),
            (
                CAPABILITY_ORE_PICKING_FLOW,
                CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(1_500)),
            ),
        ]),
        thresholds(),
        vec![
            mass_flow_condition_curve(
                CAPABILITY_COBBING_FLOW,
                500_000,
                MassFlow::from_milligrams_per_second(625),
            ),
            mass_flow_condition_curve(
                CAPABILITY_ORE_PICKING_FLOW,
                500_000,
                MassFlow::from_milligrams_per_second(750),
            ),
        ],
    )
    .with_assembly_component_maintenance(CommodityKey::new(MATERIAL_STONE, FORM_TOOL))
}
