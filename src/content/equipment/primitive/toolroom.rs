//! Low-tech abrasive toolroom equipment for recovering worn stone service components.

use crate::capability::CapabilityValue;
use crate::content::capabilities::{
    CAPABILITY_POWERED_STONE_GRINDING_FLOW, CAPABILITY_STONE_GRINDING_FLOW,
};
use crate::content::crafted_parts::{STONE_FLYWHEEL_MASS, STONE_GRINDSTONE_WHEEL_MASS};
use crate::content::materials::{
    FORM_BOARD, FORM_FLYWHEEL, FORM_GRINDSTONE_WHEEL, FORM_HANDLE, MATERIAL_STONE, MATERIAL_WOOD,
};
use crate::core::quantity::{Mass, MassFlow};
use crate::equipment::{EquipmentDefinition, EquipmentUpgradeProfile};
use crate::material::{CommodityKey, MaterialAssemblyProfile, MaterialInputSpec};

use super::super::authoring::{
    EquipmentDefinitionAuthoringExt, assembled_definition_with_condition_curves,
    mass_flow_condition_curve, profile, thresholds,
};
use super::super::{EQUIPMENT_TIMBER_FLYWHEEL_GRINDING_BENCH, EQUIPMENT_TIMBER_TREADLE_GRINDSTONE};
use super::copper_reinforcement_input;

/// Foot-driven abrasive wheel for reclaiming worn stone service stock.
///
/// This is deliberately a toolroom machine, not an ore grinder. Its narrow capability converts a
/// purpose-made wheel and timber frame into better service-part recovery while first-time tool
/// knapping remains the zero-investment primitive route.
pub(super) fn timber_treadle_grindstone() -> EquipmentDefinition {
    assembled_definition_with_condition_curves(
        EQUIPMENT_TIMBER_TREADLE_GRINDSTONE,
        "timber treadle grindstone",
        MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_STONE, FORM_GRINDSTONE_WHEEL),
                STONE_GRINDSTONE_WHEEL_MASS,
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
                Mass::from_milligrams(1_600_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(400_000),
            ),
        ]),
        profile([(
            CAPABILITY_STONE_GRINDING_FLOW,
            CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(10_000)),
        )]),
        thresholds(),
        vec![mass_flow_condition_curve(
            CAPABILITY_STONE_GRINDING_FLOW,
            500_000,
            MassFlow::from_milligrams_per_second(5_000),
        )],
    )
    .with_assembly_component_maintenance(CommodityKey::new(MATERIAL_STONE, FORM_GRINDSTONE_WHEEL))
}

/// Flywheel and copper-bearing upgrade for finite-work abrasive dressing.
///
/// The treadle capability remains available during an energy shortage; stored work only delegates
/// the same learned recovery transforms and cannot substitute for ore grinding.
pub(super) fn timber_flywheel_grinding_bench() -> EquipmentDefinition {
    assembled_definition_with_condition_curves(
        EQUIPMENT_TIMBER_FLYWHEEL_GRINDING_BENCH,
        "flywheel-driven toolroom grindstone",
        MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_STONE, FORM_GRINDSTONE_WHEEL),
                STONE_GRINDSTONE_WHEEL_MASS,
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
                Mass::from_milligrams(3_200_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(800_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
                STONE_FLYWHEEL_MASS,
            ),
            copper_reinforcement_input(),
        ]),
        profile([
            (
                CAPABILITY_STONE_GRINDING_FLOW,
                CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(15_000)),
            ),
            (
                CAPABILITY_POWERED_STONE_GRINDING_FLOW,
                CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(40_000)),
            ),
        ]),
        thresholds(),
        vec![
            mass_flow_condition_curve(
                CAPABILITY_STONE_GRINDING_FLOW,
                500_000,
                MassFlow::from_milligrams_per_second(7_500),
            ),
            mass_flow_condition_curve(
                CAPABILITY_POWERED_STONE_GRINDING_FLOW,
                500_000,
                MassFlow::from_milligrams_per_second(20_000),
            ),
        ],
    )
    .with_assembly_component_maintenance(CommodityKey::new(MATERIAL_STONE, FORM_GRINDSTONE_WHEEL))
    .with_upgrade_profile(EquipmentUpgradeProfile::new(
        EQUIPMENT_TIMBER_TREADLE_GRINDSTONE,
        MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
                Mass::from_milligrams(1_600_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(400_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
                STONE_FLYWHEEL_MASS,
            ),
            copper_reinforcement_input(),
        ]),
    ))
}
