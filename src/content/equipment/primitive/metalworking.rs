//! Human-powered settlement metalworking equipment for repeated cold-forging work.

use crate::capability::CapabilityValue;
use crate::content::capabilities::{
    CAPABILITY_COPPER_HAMMERING_FLOW, CAPABILITY_COPPER_WIRE_DRAWING_FLOW,
    CAPABILITY_POWERED_COPPER_HAMMERING_FLOW, CAPABILITY_POWERED_COPPER_WIRE_DRAWING_FLOW,
};
use crate::content::crafted_parts::{STONE_DRAWPLATE_MASS, STONE_FLYWHEEL_MASS};
use crate::content::materials::{
    FORM_BOARD, FORM_DRAWPLATE, FORM_FLYWHEEL, FORM_HANDLE, FORM_REINFORCEMENT, FORM_TOOL,
    MATERIAL_COPPER, MATERIAL_STONE, MATERIAL_WOOD,
};
use crate::core::quantity::{Mass, MassFlow};
use crate::equipment::{EquipmentDefinition, EquipmentUpgradeProfile};
use crate::material::{CommodityKey, MaterialAssemblyProfile, MaterialInputSpec};

use super::super::authoring::{
    EquipmentDefinitionAuthoringExt, assembled_definition_with_condition_curves,
    mass_flow_condition_curve, profile, thresholds,
};
use super::super::{
    EQUIPMENT_FLYWHEEL_WIRE_DRAWBENCH, EQUIPMENT_TIMBER_HELVE_HAMMER,
    EQUIPMENT_TIMBER_TREADLE_HAMMER, EQUIPMENT_TIMBER_WIRE_DRAWBENCH,
};
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

/// Bench-mounted stone drawplate and capstan for repeated copper conductor work.
///
/// This specialist deliberately does not share the generic hammering capability. Its value is
/// narrow: it turns prepared reinforcement into conductor with better throughput and no trimming
/// loss, while the replaceable polished stone die creates a real maintenance loop. One-off
/// electrical work can still use the existing hammer route instead of paying this setup cost.
pub(super) fn timber_wire_drawbench() -> EquipmentDefinition {
    assembled_definition_with_condition_curves(
        EQUIPMENT_TIMBER_WIRE_DRAWBENCH,
        "timber copper wire drawbench",
        MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_STONE, FORM_DRAWPLATE),
                STONE_DRAWPLATE_MASS,
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
                Mass::from_milligrams(2_400_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(400_000),
            ),
            copper_reinforcement_input(),
        ]),
        profile([(
            CAPABILITY_COPPER_WIRE_DRAWING_FLOW,
            CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(600)),
        )]),
        thresholds(),
        vec![mass_flow_condition_curve(
            CAPABILITY_COPPER_WIRE_DRAWING_FLOW,
            500_000,
            MassFlow::from_milligrams_per_second(300),
        )],
    )
    .with_assembly_component_maintenance(CommodityKey::new(MATERIAL_STONE, FORM_DRAWPLATE))
}

/// Flywheel-driven drawbench that delegates repetitive conductor drawing to finite stored work.
///
/// The upgrade retains the manual capstan path while adding a faster autonomous spindle. It does
/// not change the transform or create a second yield rule: both manual and powered drawing preserve
/// the full copper charge, and the powered route simply exchanges player attention for explicit
/// mechanical energy and additional workshop material.
pub(super) fn flywheel_wire_drawbench() -> EquipmentDefinition {
    assembled_definition_with_condition_curves(
        EQUIPMENT_FLYWHEEL_WIRE_DRAWBENCH,
        "flywheel-driven copper wire drawbench",
        MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_STONE, FORM_DRAWPLATE),
                STONE_DRAWPLATE_MASS,
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
                STONE_FLYWHEEL_MASS,
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
                Mass::from_milligrams(4_000_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(800_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
                Mass::from_milligrams(40_000),
            ),
        ]),
        profile([
            (
                CAPABILITY_COPPER_WIRE_DRAWING_FLOW,
                CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(800)),
            ),
            (
                CAPABILITY_POWERED_COPPER_WIRE_DRAWING_FLOW,
                CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(2_000)),
            ),
        ]),
        thresholds(),
        vec![
            mass_flow_condition_curve(
                CAPABILITY_COPPER_WIRE_DRAWING_FLOW,
                500_000,
                MassFlow::from_milligrams_per_second(400),
            ),
            mass_flow_condition_curve(
                CAPABILITY_POWERED_COPPER_WIRE_DRAWING_FLOW,
                500_000,
                MassFlow::from_milligrams_per_second(1_000),
            ),
        ],
    )
    .with_assembly_component_maintenance(CommodityKey::new(MATERIAL_STONE, FORM_DRAWPLATE))
    .with_upgrade_profile(EquipmentUpgradeProfile::new(
        EQUIPMENT_TIMBER_WIRE_DRAWBENCH,
        MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
                STONE_FLYWHEEL_MASS,
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
                Mass::from_milligrams(1_600_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(400_000),
            ),
            copper_reinforcement_input(),
        ]),
    ))
}
