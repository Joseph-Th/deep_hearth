//! Primitive woodworking tools and stations that turn repeated timber shaping into durable investment.

use crate::capability::CapabilityValue;
use crate::core::quantity::{Mass, MassFlow};
use crate::equipment::EquipmentDefinition;
use crate::material::{CommodityKey, MaterialAssemblyProfile, MaterialInputSpec};

use crate::content::capabilities::{CAPABILITY_SAWING_FLOW, CAPABILITY_WOODWORKING_FLOW};
use crate::content::materials::{
    FORM_BOARD, FORM_HANDLE, FORM_SAW_BLADE, FORM_SCRAP, FORM_TOOL, MATERIAL_COPPER,
    MATERIAL_STONE, MATERIAL_WOOD,
};

use super::super::authoring::{
    component_maintenance, mass_flow_condition_curve, profile, thresholds,
};
use super::super::{
    EQUIPMENT_COPPER_REINFORCED_WOODWORKING_ADZE, EQUIPMENT_STONE_WOODWORKING_ADZE,
    EQUIPMENT_TIMBER_FRAME_SAW_BENCH,
};
use super::{copper_reinforcement_input, copper_upgrade};

/// Stone edge and long handle for controlled splitting/hewing of boards from logs. The tool does
/// not improve material yield; it buys player attention while preserving the same explicit chips.
pub(super) fn stone_woodworking_adze() -> EquipmentDefinition {
    EquipmentDefinition::new_with_capability_condition_curves(
        EQUIPMENT_STONE_WOODWORKING_ADZE,
        "hafted stone woodworking adze",
        Mass::from_milligrams(1_000_000),
        profile([(
            CAPABILITY_WOODWORKING_FLOW,
            CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(10_000)),
        )]),
        thresholds(),
        vec![mass_flow_condition_curve(
            CAPABILITY_WOODWORKING_FLOW,
            500_000,
            MassFlow::from_milligrams_per_second(5_000),
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

/// Copper edge reinforcement doubles pristine shaping throughput without discarding the stone
/// adze's embodied material or accumulated condition.
pub(super) fn copper_reinforced_woodworking_adze() -> EquipmentDefinition {
    EquipmentDefinition::new_with_capability_condition_curves(
        EQUIPMENT_COPPER_REINFORCED_WOODWORKING_ADZE,
        "copper-reinforced stone woodworking adze",
        Mass::from_milligrams(1_020_000),
        profile([(
            CAPABILITY_WOODWORKING_FLOW,
            CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(20_000)),
        )]),
        thresholds(),
        vec![mass_flow_condition_curve(
            CAPABILITY_WOODWORKING_FLOW,
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
        copper_reinforcement_input(),
    ]))
    .with_maintenance_profile(component_maintenance(
        CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
        Mass::from_milligrams(800_000),
    ))
    .with_worn_recovery_form(FORM_SCRAP)
    .with_upgrade_profile(copper_upgrade(EQUIPMENT_STONE_WOODWORKING_ADZE))
}

/// A low bench carrying a tensioned cold-worked copper blade. It is a settlement-scale timber
/// investment rather than an adze replacement: the dedicated ripping process improves both board
/// recovery and attention cost, while blade wear creates a recurring copper-service obligation.
pub(super) fn timber_frame_saw_bench() -> EquipmentDefinition {
    EquipmentDefinition::new_with_capability_condition_curves(
        EQUIPMENT_TIMBER_FRAME_SAW_BENCH,
        "timber frame saw bench",
        Mass::from_milligrams(2_654_000),
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
    .with_assembly_profile(MaterialAssemblyProfile::new(vec![
        MaterialInputSpec::pure(
            CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
            Mass::from_milligrams(2_400_000),
        ),
        MaterialInputSpec::pure(
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            Mass::from_milligrams(200_000),
        ),
        MaterialInputSpec::pure(
            CommodityKey::new(MATERIAL_COPPER, FORM_SAW_BLADE),
            Mass::from_milligrams(54_000),
        ),
    ]))
    .with_maintenance_profile(component_maintenance(
        CommodityKey::new(MATERIAL_COPPER, FORM_SAW_BLADE),
        Mass::from_milligrams(54_000),
    ))
    .with_worn_recovery_form(FORM_SCRAP)
}
