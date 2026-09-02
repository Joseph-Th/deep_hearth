//! Primitive mining tools and their copper-reinforced variants.

use crate::capability::{CapabilityProfile, CapabilityValue};
use crate::core::quantity::{Mass, MassFlow, Pressure};
use crate::equipment::EquipmentDefinition;
use crate::material::{CommodityKey, MaterialAssemblyProfile, MaterialInputSpec};

use crate::content::capabilities::{
    CAPABILITY_MINING_FLOW, CAPABILITY_MINING_MAX_BATCH, CAPABILITY_MINING_MAX_HARDNESS,
};
use crate::content::materials::{
    FORM_HANDLE, FORM_SCRAP, FORM_TOOL, MATERIAL_STONE, MATERIAL_WOOD,
};

use super::super::authoring::{
    component_maintenance, mass_flow_condition_curve, profile, thresholds,
};
use super::super::{
    EQUIPMENT_COPPER_REINFORCED_GEOLOGICAL_HAMMER, EQUIPMENT_COPPER_REINFORCED_PICK,
    EQUIPMENT_COPPER_REINFORCED_STONE_QUARRY_PICK, EQUIPMENT_STONE_GEOLOGICAL_HAMMER,
    EQUIPMENT_STONE_PICK, EQUIPMENT_STONE_QUARRY_PICK,
};
use super::{copper_reinforcement_input, copper_upgrade};

pub(super) fn stone_pick() -> EquipmentDefinition {
    EquipmentDefinition::new_with_capability_condition_curves(
        EQUIPMENT_STONE_PICK,
        "knapped stone pick",
        Mass::from_milligrams(1_000_000),
        profile([
            (
                CAPABILITY_MINING_FLOW,
                CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(20_000)),
            ),
            (
                CAPABILITY_MINING_MAX_BATCH,
                CapabilityValue::Mass(Mass::from_milligrams(200_000)),
            ),
            (
                CAPABILITY_MINING_MAX_HARDNESS,
                CapabilityValue::Pressure(Pressure::from_pascals(500_000_000)),
            ),
        ]),
        thresholds(),
        vec![mass_flow_condition_curve(
            CAPABILITY_MINING_FLOW,
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
    ]))
    .with_maintenance_profile(component_maintenance(
        CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
        Mass::from_milligrams(800_000),
    ))
    .with_worn_recovery_form(FORM_SCRAP)
}

pub(super) fn copper_reinforced_pick() -> EquipmentDefinition {
    EquipmentDefinition::new_with_capability_condition_curves(
        EQUIPMENT_COPPER_REINFORCED_PICK,
        "copper-reinforced stone pick",
        Mass::from_milligrams(1_020_000),
        profile([
            (
                CAPABILITY_MINING_FLOW,
                CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(30_000)),
            ),
            (
                CAPABILITY_MINING_MAX_BATCH,
                CapabilityValue::Mass(Mass::from_milligrams(300_000)),
            ),
            (
                CAPABILITY_MINING_MAX_HARDNESS,
                CapabilityValue::Pressure(Pressure::from_pascals(750_000_000)),
            ),
        ]),
        thresholds(),
        vec![mass_flow_condition_curve(
            CAPABILITY_MINING_FLOW,
            500_000,
            MassFlow::from_milligrams_per_second(15_000),
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
    .with_upgrade_profile(copper_upgrade(EQUIPMENT_STONE_PICK))
}

/// A deliberately heavy soft-rock tool. It buys bulk extraction throughput with abundant stone and
/// timber but does not raise the stone tool-head hardness ceiling, so the lighter copper-reinforced
/// pick remains the ordinary route into harder seams.
pub(super) fn stone_quarry_pick() -> EquipmentDefinition {
    EquipmentDefinition::new_with_capability_condition_curves(
        EQUIPMENT_STONE_QUARRY_PICK,
        "heavy stone quarry pick",
        Mass::from_milligrams(2_000_000),
        profile([
            (
                CAPABILITY_MINING_FLOW,
                CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(35_000)),
            ),
            (
                CAPABILITY_MINING_MAX_BATCH,
                CapabilityValue::Mass(Mass::from_milligrams(500_000)),
            ),
            (
                CAPABILITY_MINING_MAX_HARDNESS,
                CapabilityValue::Pressure(Pressure::from_pascals(500_000_000)),
            ),
        ]),
        thresholds(),
        vec![mass_flow_condition_curve(
            CAPABILITY_MINING_FLOW,
            500_000,
            MassFlow::from_milligrams_per_second(17_500),
        )],
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

pub(super) fn copper_reinforced_stone_quarry_pick() -> EquipmentDefinition {
    EquipmentDefinition::new_with_capability_condition_curves(
        EQUIPMENT_COPPER_REINFORCED_STONE_QUARRY_PICK,
        "copper-reinforced heavy quarry pick",
        Mass::from_milligrams(2_020_000),
        profile([
            (
                CAPABILITY_MINING_FLOW,
                CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(45_000)),
            ),
            (
                CAPABILITY_MINING_MAX_BATCH,
                CapabilityValue::Mass(Mass::from_milligrams(750_000)),
            ),
            (
                CAPABILITY_MINING_MAX_HARDNESS,
                CapabilityValue::Pressure(Pressure::from_pascals(600_000_000)),
            ),
        ]),
        thresholds(),
        vec![mass_flow_condition_curve(
            CAPABILITY_MINING_FLOW,
            500_000,
            MassFlow::from_milligrams_per_second(22_500),
        )],
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
    .with_upgrade_profile(copper_upgrade(EQUIPMENT_STONE_QUARRY_PICK))
}

/// Portable sampling hammer used to cut repeatable chips for detailed geological observations.
/// Its value is not a generic prospecting score: the labor method explicitly authors this physical
/// instrument as an accepted tool and owns the information quality of the resulting observation.
pub(super) fn stone_geological_hammer() -> EquipmentDefinition {
    EquipmentDefinition::new(
        EQUIPMENT_STONE_GEOLOGICAL_HAMMER,
        "stone geological sampling hammer",
        Mass::from_milligrams(650_000),
        CapabilityProfile::default(),
        thresholds(),
    )
    .with_assembly_profile(MaterialAssemblyProfile::new(vec![
        MaterialInputSpec::pure(
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            Mass::from_milligrams(500_000),
        ),
        MaterialInputSpec::pure(
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            Mass::from_milligrams(150_000),
        ),
    ]))
    .with_maintenance_profile(component_maintenance(
        CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
        Mass::from_milligrams(500_000),
    ))
    .with_worn_recovery_form(FORM_SCRAP)
}

pub(super) fn copper_reinforced_geological_hammer() -> EquipmentDefinition {
    EquipmentDefinition::new(
        EQUIPMENT_COPPER_REINFORCED_GEOLOGICAL_HAMMER,
        "copper-reinforced geological sampling hammer",
        Mass::from_milligrams(670_000),
        CapabilityProfile::default(),
        thresholds(),
    )
    .with_assembly_profile(MaterialAssemblyProfile::new(vec![
        MaterialInputSpec::pure(
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            Mass::from_milligrams(500_000),
        ),
        MaterialInputSpec::pure(
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            Mass::from_milligrams(150_000),
        ),
        copper_reinforcement_input(),
    ]))
    .with_maintenance_profile(component_maintenance(
        CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
        Mass::from_milligrams(500_000),
    ))
    .with_worn_recovery_form(FORM_SCRAP)
    .with_upgrade_profile(copper_upgrade(EQUIPMENT_STONE_GEOLOGICAL_HAMMER))
}
