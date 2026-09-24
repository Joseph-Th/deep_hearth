//! Primitive woodworking tools and stations that turn repeated timber shaping into durable investment.

use crate::capability::CapabilityValue;
use crate::core::quantity::{Mass, MassFlow};
use crate::equipment::{EquipmentDefinition, EquipmentUpgradeProfile};
use crate::material::{CommodityKey, MaterialAssemblyProfile, MaterialInputSpec};

use crate::content::capabilities::{
    CAPABILITY_POWERED_SAWING_FLOW, CAPABILITY_POWERED_WOOD_TURNING_FLOW, CAPABILITY_SAWING_FLOW,
    CAPABILITY_WOOD_TURNING_FLOW, CAPABILITY_WOODWORKING_FLOW,
};
use crate::content::crafted_parts::{COPPER_SAW_BLADE_MASS, STONE_FLYWHEEL_MASS};
use crate::content::materials::{
    FORM_BOARD, FORM_FLYWHEEL, FORM_HANDLE, FORM_SAW_BLADE, FORM_TOOL, MATERIAL_COPPER,
    MATERIAL_STONE, MATERIAL_WOOD,
};

use super::super::authoring::{
    EquipmentDefinitionAuthoringExt, assembled_definition_with_condition_curves,
    mass_flow_condition_curve, profile, thresholds,
};
use super::super::{
    EQUIPMENT_COPPER_REINFORCED_WOODWORKING_ADZE, EQUIPMENT_STONE_WOODWORKING_ADZE,
    EQUIPMENT_TIMBER_FLYWHEEL_LATHE, EQUIPMENT_TIMBER_FRAME_SAW_BENCH,
    EQUIPMENT_TIMBER_SASH_SAWMILL, EQUIPMENT_TIMBER_SPRING_POLE_LATHE,
};
use super::{copper_reinforcement_input, copper_upgrade};

/// Stone edge and long handle for controlled splitting/hewing of boards from logs. The tool does
/// not improve material yield; it buys player attention while preserving the same explicit chips.
/// At half condition it retains three quarters of pristine throughput. Wear still prices service,
/// but repeated work must repay the tool rather than converge on equipment-free shaping cost.
pub(super) fn stone_woodworking_adze() -> EquipmentDefinition {
    assembled_definition_with_condition_curves(
        EQUIPMENT_STONE_WOODWORKING_ADZE,
        "hafted stone woodworking adze",
        MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
                Mass::from_milligrams(800_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(200_000),
            ),
        ]),
        profile([(
            CAPABILITY_WOODWORKING_FLOW,
            CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(10_000)),
        )]),
        thresholds(),
        vec![mass_flow_condition_curve(
            CAPABILITY_WOODWORKING_FLOW,
            500_000,
            MassFlow::from_milligrams_per_second(7_500),
        )],
    )
    .with_assembly_component_maintenance(CommodityKey::new(MATERIAL_STONE, FORM_TOOL))
}

/// A spring-pole lathe that turns repeated round timber components without changing their yield.
///
/// The reciprocating treadle keeps both hands on the cutter and gives handles and flywheels a
/// dedicated machine role instead of letting the general-purpose adze accelerate every timber
/// shape. It remains direct player work, so the machine repays its frame through reduced attention
/// without creating hidden stored energy or autonomous production.
pub(super) fn timber_spring_pole_lathe() -> EquipmentDefinition {
    assembled_definition_with_condition_curves(
        EQUIPMENT_TIMBER_SPRING_POLE_LATHE,
        "timber spring-pole lathe",
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
                CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
                Mass::from_milligrams(800_000),
            ),
        ]),
        profile([(
            CAPABILITY_WOOD_TURNING_FLOW,
            CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(25_000)),
        )]),
        thresholds(),
        vec![mass_flow_condition_curve(
            CAPABILITY_WOOD_TURNING_FLOW,
            500_000,
            MassFlow::from_milligrams_per_second(12_500),
        )],
    )
    .with_assembly_component_maintenance(CommodityKey::new(MATERIAL_STONE, FORM_TOOL))
}

/// Continuous-rotation timber lathe built around a stone flywheel and copper bearing strap.
///
/// The upgraded frame can still be treadled directly, but a finite mechanical store can turn the
/// spindle unattended. This deliberately automates only the already-learned handle and flywheel
/// transforms: it does not become a generic saw, drill, or hewing station.
pub(super) fn timber_flywheel_lathe() -> EquipmentDefinition {
    assembled_definition_with_condition_curves(
        EQUIPMENT_TIMBER_FLYWHEEL_LATHE,
        "flywheel-driven timber lathe",
        MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
                Mass::from_milligrams(3_200_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(800_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
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
                CAPABILITY_WOOD_TURNING_FLOW,
                CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(35_000)),
            ),
            (
                CAPABILITY_POWERED_WOOD_TURNING_FLOW,
                CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(100_000)),
            ),
        ]),
        thresholds(),
        vec![
            mass_flow_condition_curve(
                CAPABILITY_WOOD_TURNING_FLOW,
                500_000,
                MassFlow::from_milligrams_per_second(17_500),
            ),
            mass_flow_condition_curve(
                CAPABILITY_POWERED_WOOD_TURNING_FLOW,
                500_000,
                MassFlow::from_milligrams_per_second(50_000),
            ),
        ],
    )
    .with_assembly_component_maintenance(CommodityKey::new(MATERIAL_STONE, FORM_TOOL))
    .with_upgrade_profile(EquipmentUpgradeProfile::new(
        EQUIPMENT_TIMBER_SPRING_POLE_LATHE,
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

/// A flywheel-driven sash saw for settlement lumber runs. It preserves the frame-saw transform
/// and its 90% board recovery, but moves repetitive stroke work from player attention into finite
/// mechanical energy. The larger frame, shafting, copper bearing strap, and replaceable blade make
/// it a campaign investment rather than a free upgrade over the portable bench.
pub(super) fn timber_sash_sawmill() -> EquipmentDefinition {
    assembled_definition_with_condition_curves(
        EQUIPMENT_TIMBER_SASH_SAWMILL,
        "timber sash sawmill",
        MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
                Mass::from_milligrams(4_000_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(800_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_COPPER, FORM_SAW_BLADE),
                COPPER_SAW_BLADE_MASS,
            ),
            copper_reinforcement_input(),
        ]),
        profile([
            (
                CAPABILITY_SAWING_FLOW,
                CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(40_000)),
            ),
            (
                CAPABILITY_POWERED_SAWING_FLOW,
                CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(100_000)),
            ),
        ]),
        thresholds(),
        vec![
            mass_flow_condition_curve(
                CAPABILITY_SAWING_FLOW,
                500_000,
                MassFlow::from_milligrams_per_second(20_000),
            ),
            mass_flow_condition_curve(
                CAPABILITY_POWERED_SAWING_FLOW,
                500_000,
                MassFlow::from_milligrams_per_second(50_000),
            ),
        ],
    )
    .with_assembly_component_maintenance(CommodityKey::new(MATERIAL_COPPER, FORM_SAW_BLADE))
    .with_upgrade_profile(EquipmentUpgradeProfile::new(
        EQUIPMENT_TIMBER_FRAME_SAW_BENCH,
        MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
                Mass::from_milligrams(2_400_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(600_000),
            ),
            copper_reinforcement_input(),
        ]),
    ))
}

/// Copper edge reinforcement doubles pristine shaping throughput without discarding the stone
/// adze's embodied material or accumulated condition. It retains the same relative wear curve.
pub(super) fn copper_reinforced_woodworking_adze() -> EquipmentDefinition {
    assembled_definition_with_condition_curves(
        EQUIPMENT_COPPER_REINFORCED_WOODWORKING_ADZE,
        "copper-reinforced stone woodworking adze",
        MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
                Mass::from_milligrams(800_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(200_000),
            ),
            copper_reinforcement_input(),
        ]),
        profile([(
            CAPABILITY_WOODWORKING_FLOW,
            CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(20_000)),
        )]),
        thresholds(),
        vec![mass_flow_condition_curve(
            CAPABILITY_WOODWORKING_FLOW,
            500_000,
            MassFlow::from_milligrams_per_second(15_000),
        )],
    )
    .with_assembly_component_maintenance(CommodityKey::new(MATERIAL_STONE, FORM_TOOL))
    .with_upgrade_profile(copper_upgrade(EQUIPMENT_STONE_WOODWORKING_ADZE))
}

/// A low bench carrying a tensioned cold-worked copper blade. It is a settlement-scale timber
/// investment rather than an adze replacement: the dedicated ripping process improves both board
/// recovery and attention cost, while blade wear creates a recurring copper-service obligation.
/// The open frame is deliberately light: two adze-shaped board batches plus a handle member keep
/// setup timber near three logs so long board pipelines can repay the frame in timber as well as
/// attention, while short jobs still correctly favor the adze.
pub(super) fn timber_frame_saw_bench() -> EquipmentDefinition {
    assembled_definition_with_condition_curves(
        EQUIPMENT_TIMBER_FRAME_SAW_BENCH,
        "timber frame saw bench",
        MaterialAssemblyProfile::new(vec![
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
                Mass::from_milligrams(1_600_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(200_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_COPPER, FORM_SAW_BLADE),
                COPPER_SAW_BLADE_MASS,
            ),
        ]),
        profile([(
            CAPABILITY_SAWING_FLOW,
            CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(40_000)),
        )]),
        thresholds(),
        vec![mass_flow_condition_curve(
            CAPABILITY_SAWING_FLOW,
            500_000,
            MassFlow::from_milligrams_per_second(20_000),
        )],
    )
    .with_assembly_component_maintenance(CommodityKey::new(MATERIAL_COPPER, FORM_SAW_BLADE))
}
