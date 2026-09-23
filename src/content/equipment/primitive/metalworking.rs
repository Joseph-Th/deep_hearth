//! Human-powered settlement metalworking equipment for repeated cold-forging work.

use crate::capability::CapabilityValue;
use crate::content::capabilities::{
    CAPABILITY_COPPER_HAMMERING_FLOW, CAPABILITY_POWERED_COPPER_HAMMERING_FLOW,
};
use crate::content::materials::{
    FORM_BOARD, FORM_HANDLE, FORM_TOOL, MATERIAL_STONE, MATERIAL_WOOD,
};
use crate::core::quantity::{Mass, MassFlow};
use crate::equipment::{EquipmentDefinition, EquipmentUpgradeProfile};
use crate::material::{CommodityKey, MaterialAssemblyProfile, MaterialInputSpec};

use super::super::authoring::{
    EquipmentDefinitionAuthoringExt, assembled_definition_with_condition_curves,
    mass_flow_condition_curve, profile, thresholds,
};
use super::super::{EQUIPMENT_TIMBER_HELVE_HAMMER, EQUIPMENT_TIMBER_TREADLE_HAMMER};
use super::copper_reinforcement_input;

/// A foot-operated forging hammer with a timber frame, stone hammer head, and long treadle.
///
/// This is deliberately not a mechanically networked trip hammer. The player remains the power
/// source, so the station can live inside the current direct-labor crafting owner without inventing
/// shafts or hidden stored work. Its purpose is to turn repeated copper hammering into a durable
/// settlement investment while preserving the equipment-free hand-work fallback.
pub(super) fn timber_treadle_hammer() -> EquipmentDefinition {
    assembled_definition_with_condition_curves(
        EQUIPMENT_TIMBER_TREADLE_HAMMER,
        "timber treadle forging hammer",
        MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
                Mass::from_milligrams(3_200_000),
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
        profile([(
            CAPABILITY_COPPER_HAMMERING_FLOW,
            CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(400)),
        )]),
        thresholds(),
        vec![mass_flow_condition_curve(
            CAPABILITY_COPPER_HAMMERING_FLOW,
            500_000,
            MassFlow::from_milligrams_per_second(200),
        )],
    )
    .with_assembly_component_maintenance(CommodityKey::new(MATERIAL_STONE, FORM_TOOL))
}

/// A flywheel-driven helve hammer for repeated cold copper work. The stone head and timber frame
/// remain serviceable with ordinary materials, while one small copper bearing reinforcement ties
/// the machine to the copper-era workshop it accelerates. Unlike the treadle hammer it does not
/// consume player attention while striking; finite mechanical work and equipment wear are the cost.
pub(super) fn timber_helve_hammer() -> EquipmentDefinition {
    assembled_definition_with_condition_curves(
        EQUIPMENT_TIMBER_HELVE_HAMMER,
        "timber helve hammer",
        MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
                Mass::from_milligrams(4_000_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
                Mass::from_milligrams(800_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(800_000),
            ),
            copper_reinforcement_input(),
        ]),
        profile([
            (
                CAPABILITY_COPPER_HAMMERING_FLOW,
                CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(400)),
            ),
            (
                CAPABILITY_POWERED_COPPER_HAMMERING_FLOW,
                CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(1_500)),
            ),
        ]),
        thresholds(),
        vec![
            mass_flow_condition_curve(
                CAPABILITY_COPPER_HAMMERING_FLOW,
                500_000,
                MassFlow::from_milligrams_per_second(200),
            ),
            mass_flow_condition_curve(
                CAPABILITY_POWERED_COPPER_HAMMERING_FLOW,
                500_000,
                MassFlow::from_milligrams_per_second(750),
            ),
        ],
    )
    .with_assembly_component_maintenance(CommodityKey::new(MATERIAL_STONE, FORM_TOOL))
    .with_upgrade_profile(EquipmentUpgradeProfile::new(
        EQUIPMENT_TIMBER_TREADLE_HAMMER,
        MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
                Mass::from_milligrams(800_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(400_000),
            ),
            copper_reinforcement_input(),
        ]),
    ))
}
